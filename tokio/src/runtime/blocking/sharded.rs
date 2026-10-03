//! A sharded queue for the blocking pool's tasks.
//!
//! Tasks live in `NUM_SHARDS` queues, each with its own mutex. Spawners push
//! to a shard chosen via the thread-local RNG; workers pop by scanning the
//! shards, starting from one derived from their worker id. A mask tracks which
//! shards have tasks so scans rarely lock empty shards.
//!
//! Worker lifecycle (thread spawning, parking/waking, timeouts, shutdown) is
//! coordinated by the `coord` mutex + `condvar`, using the same claim
//! protocol as the default single-mutex queue. Unlike that queue, `coord` is
//! held only for the claim-or-spawn decision and for a worker's transition
//! to idle — never around queue operations or task execution — so spawners
//! and workers contend mostly on `1/NUM_SHARDS` of a shard lock each.

use crate::loom::sync::atomic::{AtomicBool, AtomicUsize};
use crate::loom::sync::{Condvar, Mutex};
use crate::loom::thread;

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::pool::{ShutdownHandles, SpawnError, SpawnerMetrics, Task, ThreadManagementState};

/// Number of shards. Must be a power of two.
///
/// Under loom, use a small shard count to keep the state space tractable
/// while still exercising the cross-shard scanning in `pop`.
#[cfg(not(loom))]
const NUM_SHARDS: usize = 16;
#[cfg(loom)]
const NUM_SHARDS: usize = 2;

struct Shard {
    queue: VecDeque<Task>,
    /// Set (under the shard's lock) when the shard is drained for shutdown;
    /// pushes to a sealed shard are rejected. This is what guarantees that a
    /// spawner racing with shutdown cannot leave a task behind: a push
    /// either loses (rejected, task is shut down) or wins, in which case the
    /// sealer that later drains this shard collects the task.
    sealed: bool,
}

pub(super) struct ShardedImpl {
    shards: [Mutex<Shard>; NUM_SHARDS],
    /// One bit per shard, set when that shard's queue is non-empty. Only
    /// updated while holding that shard's lock, so it is exact at every
    /// lock release; unlocked loads may be stale (see `pop`).
    non_empty_mask: AtomicUsize,
    coord: Mutex<ShardedCoord>,
    condvar: Condvar,
    /// Mirror of `ThreadManagementState::shutdown`, to let spawners reject
    /// tasks without taking `coord`.
    is_shutdown: AtomicBool,
    /// Round-robin push counter, used to pick a shard when the thread-local
    /// RNG is unavailable (loom requires each execution to be deterministic).
    #[cfg(loom)]
    push_index: AtomicUsize,
}

/// State protected by `ShardedImpl::coord`. This mirrors `LockedInner`,
/// except that the queue itself lives in the shards.
struct ShardedCoord {
    /// Pending worker wakeups. A spawner claims an idle worker by
    /// decrementing `num_idle_threads` and incrementing this; a woken worker
    /// acknowledges by decrementing it. Distinguishes real wakeups from
    /// spurious ones.
    num_notify: u32,
    thread_mgmt_state: ThreadManagementState,
}

impl ShardedImpl {
    pub(super) fn new(thread_mgmt_state: ThreadManagementState) -> ShardedImpl {
        ShardedImpl {
            shards: std::array::from_fn(|_| {
                Mutex::new(Shard {
                    queue: VecDeque::new(),
                    sealed: false,
                })
            }),
            non_empty_mask: AtomicUsize::new(0),
            coord: Mutex::new(ShardedCoord {
                num_notify: 0,
                thread_mgmt_state,
            }),
            condvar: Condvar::new(),
            is_shutdown: AtomicBool::new(false),
            #[cfg(loom)]
            push_index: AtomicUsize::new(0),
        }
    }

    /// Pick a shard to push to. Use the thread-local RNG so that concurrent
    /// spawners spread across the shards.
    #[cfg(not(loom))]
    fn push_shard_index(&self) -> usize {
        crate::runtime::context::thread_rng_n(NUM_SHARDS as u32) as usize
    }

    /// Under loom the RNG would make each execution take a different path,
    /// breaking loom's requirement that executions be deterministic, so use
    /// round-robin selection instead.
    #[cfg(loom)]
    fn push_shard_index(&self) -> usize {
        self.push_index.fetch_add(1, Ordering::Relaxed) % NUM_SHARDS
    }

    /// Push a task onto one of the shards, or hand it back if the chosen
    /// shard has been sealed for shutdown.
    ///
    /// The queue-depth metric is incremented under the shard lock so that
    /// the pop that consumes this task (whose decrement is ordered after
    /// this lock's release) can never transiently wrap the counter.
    fn push(&self, task: Task, metrics: &SpawnerMetrics) -> Result<(), Task> {
        let index = self.push_shard_index();
        let mut shard = self.shards[index].lock();
        if shard.sealed {
            return Err(task);
        }
        shard.queue.push_back(task);
        metrics.inc_queue_depth();
        if shard.queue.len() == 1 {
            self.non_empty_mask.fetch_or(1 << index, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Pop a task, checking the worker's preferred shard first.
    fn pop(&self, preferred_shard: usize) -> Option<Task> {
        let mask = self.non_empty_mask.load(Ordering::Relaxed);
        if mask == 0 {
            return None;
        }

        let start = preferred_shard % NUM_SHARDS;
        for i in 0..NUM_SHARDS {
            let index = (start + i) % NUM_SHARDS;
            if mask & (1 << index) == 0 {
                continue;
            }

            let mut shard = self.shards[index].lock();
            match shard.queue.pop_front() {
                Some(task) => {
                    if shard.queue.is_empty() {
                        self.non_empty_mask
                            .fetch_and(!(1 << index), Ordering::Relaxed);
                    }
                    return Some(task);
                }
                None => {
                    // The shard was emptied (and its bit cleared) after the
                    // mask was loaded; move on to the next candidate.
                }
            }
        }

        None
    }

    /// Drain every shard, sealing each so that later pushes are rejected,
    /// and run-or-cancel the collected tasks. Called by workers during
    /// shutdown (and by `begin_shutdown` when there are no workers left to
    /// do it). Sealing is idempotent, so concurrent callers are fine.
    fn drain_and_seal(&self, metrics: &SpawnerMetrics, preferred_shard: usize) {
        let start = preferred_shard % NUM_SHARDS;
        for i in 0..NUM_SHARDS {
            let index = (start + i) % NUM_SHARDS;
            let tasks = {
                let mut shard = self.shards[index].lock();
                shard.sealed = true;
                self.non_empty_mask
                    .fetch_and(!(1 << index), Ordering::Relaxed);
                std::mem::take(&mut shard.queue)
            };
            for task in tasks {
                metrics.dec_queue_depth();
                task.shutdown_or_run_if_mandatory();
            }
        }
    }

    /// Push a task and either notify an idle worker or invoke
    /// `on_no_idle` (which is responsible for spawning a new worker if
    /// possible).
    pub(super) fn spawn_task<F>(
        &self,
        task: Task,
        metrics: &SpawnerMetrics,
        on_no_idle: F,
    ) -> Result<(), SpawnError>
    where
        F: FnOnce(&mut ThreadManagementState) -> Result<(), SpawnError>,
    {
        if self.is_shutdown.load(Ordering::Acquire) {
            // It's fine to shutdown this task (even if mandatory): it was
            // scheduled after the shutdown of the runtime began.
            task.shutdown();
            return Err(SpawnError::ShuttingDown);
        }

        // Push before taking `coord`, so spawners don't serialize on a
        // pool-wide lock held across the queue operation.
        if let Err(task) = self.push(task, metrics) {
            // The shard was already drained and sealed for shutdown: reject
            // the task, exactly as if the shutdown check above had caught it.
            task.shutdown();
            return Err(SpawnError::ShuttingDown);
        }

        let mut coord = self.coord.lock();

        if coord.thread_mgmt_state.shutdown {
            // Shutdown raced with our push, but the push beat the seal, so
            // whichever worker (or `begin_shutdown`) seals that shard is
            // guaranteed to collect the task and run it (if mandatory) or
            // shut it down. Nothing to do here.
            return Ok(());
        }

        if metrics.num_idle_threads() == 0 {
            on_no_idle(&mut coord.thread_mgmt_state)?;
        } else {
            // Claim an idle worker (see `num_notify`). Signal after
            // releasing `coord` so the woken worker doesn't immediately
            // block on it; the counter increment, made under `coord`, is
            // what guarantees the wakeup cannot be lost.
            metrics.dec_num_idle_threads();
            coord.num_notify += 1;
            drop(coord);
            self.condvar.notify_one();
        }

        Ok(())
    }

    /// Run a worker thread's main loop.
    pub(super) fn run_worker(
        &self,
        metrics: &SpawnerMetrics,
        keep_alive: Duration,
        worker_thread_id: usize,
    ) -> Option<thread::JoinHandle<()>> {
        let mut join_on_thread = None;
        let mut coord;

        'main: loop {
            // BUSY: run tasks without holding `coord`, so that spawners and
            // other workers are not blocked on this worker.
            while let Some(task) = self.pop(worker_thread_id) {
                metrics.dec_queue_depth();
                task.run();
            }

            coord = self.coord.lock();

            // Re-check the shards under `coord` before going idle: a task
            // may have been pushed after the scan above, its spawner seeing
            // this worker as busy and so neither notifying nor spawning.
            // `coord` orders this re-check against every claim-or-spawn
            // decision: a spawner that decided first pushed (and set the
            // mask bit) before its `coord` critical section, so the task is
            // visible here; one that decides later sees this worker counted
            // idle and claims it.
            if let Some(task) = self.pop(worker_thread_id) {
                metrics.dec_queue_depth();
                drop(coord);
                task.run();
                continue 'main;
            }

            // IDLE
            metrics.inc_num_idle_threads();

            while !coord.thread_mgmt_state.shutdown {
                let lock_result = self.condvar.wait_timeout(coord, keep_alive).unwrap();

                coord = lock_result.0;
                let timeout_result = lock_result.1;

                if coord.num_notify != 0 {
                    // A legitimate wakeup; the spawner already decremented
                    // `num_idle_threads` on this worker's behalf.
                    coord.num_notify -= 1;
                    drop(coord);
                    continue 'main;
                }

                // Even if the condvar "timed out", if the pool is
                // entering the shutdown phase, we want to perform
                // the cleanup logic.
                if !coord.thread_mgmt_state.shutdown && timeout_result.timed_out() {
                    join_on_thread = coord.thread_mgmt_state.worker_timed_out(worker_thread_id);

                    break 'main;
                }

                // Spurious wakeup detected, go back to sleep.
            }

            // The pool is shutting down: drain and seal the shards, so that
            // a spawner racing with shutdown either gets its task collected
            // here or gets its push rejected (see `spawn_task`).
            drop(coord);
            self.drain_and_seal(metrics, worker_thread_id);

            coord = self.coord.lock();
            break 'main;
        }

        // Thread exit
        metrics.dec_num_threads();

        // Unlike `LockedImpl`, this worker is always counted in
        // `num_idle_threads` here: both loop exits (timeout and shutdown)
        // are reached after the IDLE transition without a spawner having
        // claimed this worker.
        let prev_idle = metrics.dec_num_idle_threads();
        assert_ne!(
            prev_idle, 0,
            "`num_idle_threads` underflowed on thread exit"
        );

        if coord.thread_mgmt_state.shutdown && metrics.num_threads() == 0 {
            self.condvar.notify_one();
        }

        drop(coord);

        join_on_thread
    }

    /// Begin pool shutdown: set the shutdown flag, drop the shutdown
    /// sender, wake all waiting workers, and hand back the worker
    /// `JoinHandle`s for the caller to join.
    pub(super) fn begin_shutdown(&self, metrics: &SpawnerMetrics) -> Option<ShutdownHandles> {
        let mut coord = self.coord.lock();
        let handles = coord.thread_mgmt_state.begin_shutdown()?;
        self.is_shutdown.store(true, Ordering::Release);
        self.condvar.notify_all();

        // Every live worker seals the shards on its way out, but if there
        // are no workers (and none can be spawned now that `shutdown` is
        // set), seal here so a racing spawner's push can't be stranded.
        let no_workers = metrics.num_threads() == 0;
        drop(coord);
        if no_workers {
            self.drain_and_seal(metrics, 0);
        }

        Some(handles)
    }
}
