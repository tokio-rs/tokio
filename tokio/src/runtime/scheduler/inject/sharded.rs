//! A sharded inject queue for the multi-thread scheduler.
//!
//! Tasks live in a power-of-two number of shards, each with its own mutex,
//! closed bit, and length atomic, so concurrent pushers contend on
//! `1/num_shards` of a shard lock each instead of a single queue-wide mutex.
//! Each thread sticks to one randomly chosen shard: pushes go to it, and pop
//! scans start from it, covering the ring so every shard is eventually
//! drained.
//!
//! A bitmask tracks which shards are non-empty. It is only updated while
//! holding the corresponding shard's lock, so it is exact at every lock
//! release, and a single load of it gives the same snapshot emptiness
//! semantics as the single queue's length atomic: in particular, the
//! parking-worker protocol's unlocked "is work pending?" re-check reads one
//! atomic, not a staggered scan that a concurrent push could slip behind.

use super::{Pop, Shared, Synced};

use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Mutex;
use crate::runtime::task;
use crate::util::cacheline::CachePadded;

use std::sync::atomic::Ordering::Relaxed;

/// Returns this thread's sticky shard value, chosen randomly the first time
/// the thread touches a sharded queue. The value is reduced modulo the shard
/// count per queue.
#[cfg(not(loom))]
fn thread_shard_value() -> usize {
    use std::cell::Cell;

    std::thread_local! {
        static SHARD_VALUE: Cell<usize> = const { Cell::new(usize::MAX) };
    }

    SHARD_VALUE.with(|cell| {
        let mut value = cell.get();

        if value == usize::MAX {
            value = crate::runtime::context::thread_rng_n(u32::MAX) as usize;
            cell.set(value);
        }

        value
    })
}

/// Maximum number of shards. Bounded by the width of the non-empty bitmask.
///
/// Under loom, use a single shard to keep the state space tractable; each
/// added shard is another mutex for loom to interleave.
#[cfg(not(loom))]
const MAX_SHARDS: usize = 16;
#[cfg(loom)]
const MAX_SHARDS: usize = 1;

struct Shard<T: 'static> {
    shared: Shared<T>,
    synced: Mutex<Synced>,
}

pub(crate) struct ShardedInject<T: 'static> {
    /// Each shard is padded to its own cache line so that one shard's lock
    /// and length traffic does not falsely share with its neighbors'.
    shards: Box<[CachePadded<Shard<T>>]>,

    /// One bit per shard, set when that shard is non-empty. Only updated
    /// while holding that shard's lock, so it is exact at every lock
    /// release; unlocked loads may be stale, exactly like the single
    /// queue's length atomic.
    non_empty_mask: CachePadded<AtomicUsize>,
}

impl<T: 'static> ShardedInject<T> {
    pub(crate) fn new(num_workers: usize) -> ShardedInject<T> {
        let num_shards = num_workers.next_power_of_two().min(MAX_SHARDS);

        ShardedInject {
            shards: (0..num_shards)
                .map(|_| {
                    let (shared, synced) = Shared::new();
                    CachePadded::new(Shard {
                        shared,
                        synced: Mutex::new(synced),
                    })
                })
                .collect(),
            non_empty_mask: CachePadded::new(AtomicUsize::new(0)),
        }
    }

    /// Pick a shard to push to: each thread sticks to one randomly chosen
    /// shard. Distinct pushers land on distinct shards (so they do not
    /// contend), while a single pusher fills a single shard (so a popper
    /// grabbing a batch finds all of its tasks together). A worker's
    /// overflowed tasks likewise always go to that worker's own shard.
    #[cfg(not(loom))]
    fn push_shard(&self) -> usize {
        thread_shard_value() % self.shards.len()
    }

    // Under loom there is a single shard (see `MAX_SHARDS`).
    #[cfg(loom)]
    fn push_shard(&self) -> usize {
        0
    }

    /// Pick the shard a pop scan starts from: each thread starts at its own
    /// sticky shard, so concurrent poppers spread across the shards, and a
    /// worker drains its own overflow first.
    #[cfg(not(loom))]
    fn scan_start(&self) -> usize {
        thread_shard_value() % self.shards.len()
    }

    #[cfg(loom)]
    fn scan_start(&self) -> usize {
        0
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.non_empty_mask.load(Relaxed) == 0
    }

    pub(crate) fn len(&self) -> usize {
        self.shards.iter().map(|shard| shard.shared.len()).sum()
    }

    /// Returns `true` only once every shard is closed: `close` closes the
    /// shards in index order, and this reads the last one, so an observer
    /// that sees the queue closed cannot race with a shard that is still
    /// accepting pushes. The shutdown drain relies on this: it runs only
    /// after workers observe the close, at which point a push to any shard
    /// either landed before that shard closed (and the drain collects it) or
    /// was rejected.
    pub(crate) fn is_closed(&self) -> bool {
        let shard = &self.shards[self.shards.len() - 1];
        let synced = shard.synced.lock();
        shard.shared.is_closed(&synced)
    }

    /// Closes every shard, returns `true` if the queue was open when the
    /// transition was made.
    pub(crate) fn close(&self) -> bool {
        let mut ret = false;

        for shard in self.shards.iter() {
            let mut synced = shard.synced.lock();
            ret |= shard.shared.close(&mut synced);
        }

        ret
    }

    /// Pushes a value into one of the shards.
    ///
    /// This does nothing if the queue is closed.
    pub(crate) fn push(&self, task: task::Notified<T>) {
        let index = self.push_shard();
        let shard = &self.shards[index];

        let mut synced = shard.synced.lock();
        let was_empty = shard.shared.is_empty();

        // safety: passing the `Synced` this shard's `Shared` was created with
        unsafe { shard.shared.push(&mut synced, task) };

        if was_empty && !shard.shared.is_empty() {
            self.non_empty_mask.fetch_or(1 << index, Relaxed);
        }
    }

    pub(crate) fn pop(&self) -> Option<task::Notified<T>> {
        let mask = self.non_empty_mask.load(Relaxed);
        if mask == 0 {
            return None;
        }

        let num_shards = self.shards.len();
        let start = self.scan_start();

        for i in 0..num_shards {
            let index = (start + i) % num_shards;
            if mask & (1 << index) == 0 {
                continue;
            }

            let shard = &self.shards[index];
            let mut synced = shard.synced.lock();

            // safety: passing the `Synced` this shard's `Shared` was created
            // with
            let task = unsafe { shard.shared.pop(&mut synced) };

            if shard.shared.is_empty() {
                self.non_empty_mask.fetch_and(!(1 << index), Relaxed);
            }

            if task.is_some() {
                return task;
            }

            // The shard was emptied (and its bit cleared) after the mask was
            // loaded; move on to the next candidate.
        }

        None
    }

    /// Pushes several values into a single shard, preserving their order.
    ///
    /// This does nothing if the queue is closed.
    pub(crate) fn push_batch<I>(&self, iter: I)
    where
        I: Iterator<Item = task::Notified<T>>,
    {
        let index = self.push_shard();
        let shard = &self.shards[index];

        // safety: passing the `Synced` this shard's `Shared` was created with
        let became_non_empty = unsafe { shard.shared.push_batch(&shard.synced, iter) };

        // The bit is set after the shard's lock is released, so the batch is
        // briefly invisible to other threads' mask loads. That is benign: it
        // is set before this thread's subsequent worker notification, which
        // is the ordering the wakeup protocol relies on, and no other thread
        // clears the bit in the window (`pop` only visits shards whose bit
        // is set).
        if became_non_empty {
            self.non_empty_mask.fetch_or(1 << index, Relaxed);
        }
    }

    /// Pops up to `n` values from the queue, scanning as many shards as it
    /// takes to fill the batch. `f` is called once per shard that
    /// contributes, with an iterator over that shard's values; it may not be
    /// called at all if every shard is empty.
    pub(crate) fn pop_n(&self, n: usize, mut f: impl FnMut(Pop<'_, T>)) {
        let mask = self.non_empty_mask.load(Relaxed);
        if mask == 0 {
            return;
        }

        let num_shards = self.shards.len();
        let start = self.scan_start();
        let mut remaining = n;

        for i in 0..num_shards {
            if remaining == 0 {
                return;
            }

            let index = (start + i) % num_shards;
            if mask & (1 << index) == 0 {
                continue;
            }

            let shard = &self.shards[index];
            let mut synced = shard.synced.lock();

            // safety: passing the `Synced` this shard's `Shared` was created
            // with
            let tasks = unsafe { shard.shared.pop_n(&mut synced, remaining) };
            remaining -= tasks.len();

            if tasks.len() > 0 {
                f(tasks);
            }

            if shard.shared.is_empty() {
                self.non_empty_mask.fetch_and(!(1 << index), Relaxed);
            }
        }
    }

    /// Pops every task from every shard into `dst`. Each shard is drained
    /// atomically with respect to concurrent pushes, but the drain as a
    /// whole is not: a task pushed concurrently may land in an
    /// already-drained shard and be missed. The taskdump tracer tolerates
    /// this, as such a task is already notified and is skipped by the trace.
    #[cfg(all(tokio_unstable, feature = "taskdump"))]
    pub(crate) fn drain_into(&self, dst: &mut Vec<task::Notified<T>>) {
        for (index, shard) in self.shards.iter().enumerate() {
            let mut synced = shard.synced.lock();

            // safety: passing the `Synced` this shard's `Shared` was created
            // with
            while let Some(task) = unsafe { shard.shared.pop(&mut synced) } {
                dst.push(task);
            }

            self.non_empty_mask.fetch_and(!(1 << index), Relaxed);
        }
    }
}
