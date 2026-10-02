//! A sharded queue for the blocking pool's tasks.
//!
//! Tasks live in `NUM_SHARDS` queues, each with its own mutex. Spawners push
//! to a shard chosen via the thread-local RNG; workers pop by scanning the
//! shards, initially starting from one derived from their worker id. Workers
//! take at most `TASKS_PER_SHARD` tasks from each shard before moving on. A mask
//! tracks which shards have tasks so scans rarely lock empty shards.
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

// Bound each worker's preference for one shard while retaining some locality.
const TASKS_PER_SHARD: usize = 8;

struct PopCursor {
    next_shard: usize,
    remaining: usize,
}

impl PopCursor {
    fn new(worker_thread_id: usize) -> Self {
        Self {
            next_shard: worker_thread_id % NUM_SHARDS,
            remaining: TASKS_PER_SHARD,
        }
    }
}

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

    /// Pop a task, advancing the worker's cursor after a bounded number of
    /// tasks from one shard so busy shards cannot starve later ones.
    fn pop(&self, cursor: &mut PopCursor) -> Option<Task> {
        let start = cursor.next_shard;
        // Rotate the snapshot so its set bits are visited in scan order,
        // skipping empty shards without checking each intervening bit.
        let mut mask = self
            .non_empty_mask
            .load(Ordering::Relaxed)
            .rotate_right(start as u32);
        while mask != 0 {
            // Bits below `start` wrap to the high end of the word. Since
            // NUM_SHARDS divides the word size, this also wraps their index.
            let index = (start + mask.trailing_zeros() as usize) % NUM_SHARDS;

            let mut shard = self.shards[index].lock();
            match shard.queue.pop_front() {
                Some(task) => {
                    if shard.queue.is_empty() {
                        self.non_empty_mask
                            .fetch_and(!(1 << index), Ordering::Relaxed);
                    }
                    if index != start {
                        cursor.remaining = TASKS_PER_SHARD;
                    }
                    cursor.remaining -= 1;
                    if cursor.remaining == 0 {
                        cursor.next_shard = (index + 1) % NUM_SHARDS;
                        cursor.remaining = TASKS_PER_SHARD;
                    } else {
                        cursor.next_shard = index;
                    }
                    return Some(task);
                }
                None => {
                    // The shard was emptied (and its bit cleared) after the
                    // mask was loaded; move on to the next candidate.
                    mask &= mask - 1;
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
        let mut cursor = PopCursor::new(worker_thread_id);

        'main: loop {
            // BUSY: run tasks without holding `coord`, so that spawners and
            // other workers are not blocked on this worker.
            while let Some(task) = self.pop(&mut cursor) {
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
            if let Some(task) = self.pop(&mut cursor) {
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

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;
    use crate::runtime::blocking::pool::Mandatory;
    use crate::runtime::blocking::schedule::BlockingSchedule;
    use crate::runtime::{task, Builder, Handle, Runtime};
    use std::collections::HashMap;
    use std::sync::mpsc;
    use std::time::Instant;

    const TIMEOUT: Duration = Duration::from_secs(10);

    // Bypass only random shard selection, leaving worker execution and its
    // queue-depth accounting intact.
    fn push_to_shard(
        handle: &Handle,
        queue: &ShardedImpl,
        metrics: &SpawnerMetrics,
        index: usize,
        f: impl FnOnce() + Send + 'static,
    ) {
        let (task, _) = task::unowned(
            async move { f() },
            BlockingSchedule::new(handle),
            task::Id::next(),
            task::SpawnLocation::capture(),
        );
        let mut shard = queue.shards[index].lock();
        assert!(!shard.sealed);
        shard
            .queue
            .push_back(Task::new(task, Mandatory::NonMandatory));
        metrics.inc_queue_depth();
        queue.non_empty_mask.fetch_or(1 << index, Ordering::Relaxed);
    }

    fn sharded_runtime() -> Runtime {
        let mut builder = Builder::new_current_thread();
        builder.sharded_blocking_queue = true;
        builder
            .max_blocking_threads(1)
            .thread_keep_alive(Duration::from_secs(60))
            .build()
            .unwrap()
    }

    // Keep the hot shard replenished, but bound the chain so a regression
    // fails an ordering assertion instead of hanging runtime shutdown.
    fn replenish_hot_shard(handle: Handle, remaining: usize, tx: mpsc::Sender<usize>) {
        let next_handle = handle.clone();
        let (queue, metrics) = handle.inner.blocking_spawner().sharded_queue();
        push_to_shard(&handle, queue, metrics, 0, move || {
            tx.send(0).unwrap();
            if remaining > 1 {
                replenish_hot_shard(next_handle, remaining - 1, tx);
            }
        });
    }

    #[test]
    fn worker_services_cold_shard_while_hot_shard_is_replenished() {
        let rt = sharded_runtime();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let blocker = rt.spawn_blocking(move || {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(TIMEOUT).unwrap();
        });
        started_rx.recv_timeout(TIMEOUT).unwrap();

        let (tx, rx) = mpsc::channel();
        let (queue, metrics) = rt.handle().inner.blocking_spawner().sharded_queue();
        let cold = NUM_SHARDS - 1;
        let cold_tx = tx.clone();
        push_to_shard(rt.handle(), queue, metrics, cold, move || {
            cold_tx.send(cold).unwrap();
        });
        replenish_hot_shard(rt.handle().clone(), 64, tx);
        release_tx.send(()).unwrap();

        let order: Vec<_> = (0..65).map(|_| rx.recv_timeout(TIMEOUT).unwrap()).collect();
        rt.block_on(blocker).unwrap();
        // With only two occupied shards, the cold task must run after at
        // most one hot batch, regardless of the blocker's chosen shard.
        assert!(
            order[..TASKS_PER_SHARD + 1].contains(&cold),
            "execution order: {order:?}"
        );
        assert_eq!(order.iter().filter(|&&index| index == 0).count(), 64);
    }

    #[test]
    fn worker_preserves_cursor_after_idle() {
        let rt = sharded_runtime();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let blocker = rt.spawn_blocking(move || {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(TIMEOUT).unwrap();
        });
        started_rx.recv_timeout(TIMEOUT).unwrap();

        let (queue, metrics) = rt.handle().inner.blocking_spawner().sharded_queue();
        let (tx, rx) = mpsc::channel();
        let previous = NUM_SHARDS / 2;
        let done_tx = tx.clone();
        push_to_shard(rt.handle(), queue, metrics, previous, move || {
            done_tx.send(previous).unwrap();
        });
        release_tx.send(()).unwrap();
        assert_eq!(rx.recv_timeout(TIMEOUT).unwrap(), previous);
        rt.block_on(blocker).unwrap();

        // Observe the real IDLE transition under its coordination lock. Keep
        // that lock until both competing shards are ready, so the assertion
        // does not depend on sleeps or producer/worker timing.
        let deadline = Instant::now() + TIMEOUT;
        let coord = loop {
            let coord = queue.coord.lock();
            if metrics.num_idle_threads() == 1 {
                break coord;
            }
            drop(coord);
            assert!(Instant::now() < deadline, "worker did not become idle");
            std::thread::yield_now();
        };
        for index in [0, previous + 1] {
            let tx = tx.clone();
            push_to_shard(rt.handle(), queue, metrics, index, move || {
                tx.send(index).unwrap();
            });
        }
        drop(coord);

        // Wake via the real spawn/claim/notify protocol. This extra task has
        // no observable output and does not affect the two tasks' order.
        let wakeup = rt.spawn_blocking(|| {});
        let first = rx.recv_timeout(TIMEOUT).unwrap();
        let second = rx.recv_timeout(TIMEOUT).unwrap();
        rt.block_on(wakeup).unwrap();
        assert_eq!([first, second], [previous + 1, 0]);
    }

    // Control shard placement so fairness does not depend on the RNG or timing.
    fn drain_order(shards: &[usize], next_shard: usize) -> Vec<(usize, usize)> {
        let rt = Builder::new_current_thread().build().unwrap();
        let queue = ShardedImpl::new(ThreadManagementState {
            shutdown: false,
            shutdown_tx: None,
            last_exiting_thread: None,
            worker_threads: HashMap::new(),
            worker_thread_index: 0,
        });
        let (tx, rx) = mpsc::channel();

        for &index in shards {
            // Leave each shard non-empty across two complete batches.
            for sequence in 0..TASKS_PER_SHARD * 2 + 1 {
                let tx = tx.clone();
                let (task, _) = task::unowned(
                    async move { tx.send((index, sequence)).unwrap() },
                    BlockingSchedule::new(rt.handle()),
                    task::Id::next(),
                    task::SpawnLocation::capture(),
                );
                queue.shards[index]
                    .lock()
                    .queue
                    .push_back(Task::new(task, Mandatory::NonMandatory));
                queue.non_empty_mask.fetch_or(1 << index, Ordering::Relaxed);
            }
        }
        drop(tx);

        let mut cursor = PopCursor::new(next_shard);
        while let Some(task) = queue.pop(&mut cursor) {
            task.run();
        }
        rx.try_iter().collect()
    }

    fn expected_order(shards: &[usize]) -> Vec<(usize, usize)> {
        let tasks = TASKS_PER_SHARD * 2 + 1;
        (0..tasks)
            .step_by(TASKS_PER_SHARD)
            .flat_map(|base| {
                shards.iter().flat_map(move |&index| {
                    (base..(base + TASKS_PER_SHARD).min(tasks))
                        .map(move |sequence| (index, sequence))
                })
            })
            .collect()
    }

    #[test]
    fn pop_rotates_through_busy_shards() {
        for start in (0..NUM_SHARDS).chain([usize::MAX]) {
            let shards: Vec<_> = (0..NUM_SHARDS)
                .map(|i| (start % NUM_SHARDS + i) % NUM_SHARDS)
                .collect();
            assert_eq!(drain_order(&shards, start), expected_order(&shards));
        }
    }

    #[test]
    fn pop_rotates_past_empty_shards() {
        for start in 0..NUM_SHARDS {
            assert!(drain_order(&[], start).is_empty());
            for first in 0..NUM_SHARDS {
                assert_eq!(drain_order(&[first], start), expected_order(&[first]));
                for second in first + 1..NUM_SHARDS {
                    let mut shards = [first, second];
                    shards.sort_by_key(|&index| (index + NUM_SHARDS - start) % NUM_SHARDS);
                    assert_eq!(drain_order(&shards, start), expected_order(&shards));
                }
            }
        }
    }
}
