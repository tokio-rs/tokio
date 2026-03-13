//! Sharded inject queue for the multi-threaded scheduler.
//!
//! A single global mutex is the dominant source of contention when many
//! external threads spawn into the runtime concurrently. `Sharded` splits
//! the inject queue into independent shards, each with its own mutex and
//! intrusive linked list. Pushes are distributed across shards using a
//! per-thread counter, so uncontended threads never touch the same lock.
//! Workers drain shards starting from their own index and rotate.

use super::{Pop, Shared, Synced};

use crate::loom::sync::atomic::AtomicBool;
use crate::loom::sync::{Mutex, MutexGuard};
use crate::runtime::task;
use crate::util::cacheline::CachePadded;

use std::sync::atomic::Ordering::{Acquire, Release};

/// Sharded inject queue.
///
/// Internally composed of `N` independent [`Shared`] / [`Synced`] pairs,
/// each protected by its own mutex and padded to avoid false sharing.
pub(crate) struct Sharded<T: 'static> {
    /// One entry per shard.
    shards: Box<[CachePadded<Shard<T>>]>,

    /// `shards.len() - 1`, used for fast modulo. Shard count is always
    /// a power of two.
    shard_mask: usize,

    /// Set once all shards have been closed. Allows `is_closed` to be
    /// checked without locking a shard.
    is_closed: AtomicBool,
}

struct Shard<T: 'static> {
    shared: Shared<T>,
    synced: Mutex<Synced>,
}

cfg_not_loom! {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicUsize as StdAtomicUsize, Ordering::Relaxed};

    /// Sentinel indicating the per-thread push shard has not been assigned.
    const UNASSIGNED: usize = usize::MAX;

    tokio_thread_local! {
        /// Per-thread home shard for push operations.
        ///
        /// Each thread sticks to one shard for cache locality: consecutive
        /// pushes from the same thread hit the same mutex and linked-list
        /// tail. Distinct threads get distinct shards (modulo collisions)
        /// via a global counter assigned on first use.
        static PUSH_SHARD: Cell<usize> = const { Cell::new(UNASSIGNED) };
    }

    /// Hands out shard indices to threads on first push. Shared across all
    /// `Sharded` instances, which is fine: it only needs to spread threads
    /// out. Uses `std` atomics directly (not loom) because shard selection
    /// has no correctness implications and loom caps shards at 1 anyway.
    static NEXT_SHARD: StdAtomicUsize = StdAtomicUsize::new(0);
}

/// Upper bound on shard count. More shards reduce push contention but
/// make `is_empty`/`len` (which scan every shard) slower, and those are
/// called in the worker hot loop. Contention drops off steeply past a
/// handful of shards, so a small cap captures the win.
///
/// Under loom, additional shards would multiply the modeled state space
/// without testing any new interleavings: each shard is an independent
/// instance of the already-loom-tested `Shared`/`Synced` pair, and the
/// cross-shard rotation is plain sequential code.
#[cfg(loom)]
const MAX_SHARDS: usize = 1;

#[cfg(not(loom))]
const MAX_SHARDS: usize = 8;

impl<T: 'static> Sharded<T> {
    /// Creates a new sharded inject queue with a shard count derived
    /// from the requested hint (rounded up to a power of two).
    pub(crate) fn new(shard_hint: usize) -> Sharded<T> {
        let num_shards = shard_hint.clamp(1, MAX_SHARDS).next_power_of_two();

        let shards = (0..num_shards)
            .map(|_| {
                let (shared, synced) = Shared::new();
                CachePadded::new(Shard {
                    shared,
                    synced: Mutex::new(synced),
                })
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Sharded {
            shards,
            shard_mask: num_shards - 1,
            is_closed: AtomicBool::new(false),
        }
    }

    /// Returns the total number of tasks across all shards.
    ///
    /// This is a sum of per-shard atomic reads and is thus an
    /// approximation under concurrent modification. With the shard
    /// count capped small, the scan is cheap.
    pub(crate) fn len(&self) -> usize {
        let mut len = 0;
        for shard in self.shards.iter() {
            len += shard.shared.len();
        }
        len
    }

    /// Returns `true` if every shard reports empty.
    pub(crate) fn is_empty(&self) -> bool {
        for shard in self.shards.iter() {
            if !shard.shared.is_empty() {
                return false;
            }
        }
        true
    }

    /// Returns `true` if `close` has been called.
    pub(crate) fn is_closed(&self) -> bool {
        self.is_closed.load(Acquire)
    }

    /// Closes all shards and prevents further pushes.
    ///
    /// Returns `true` if the queue was open when the transition was made.
    pub(crate) fn close(&self) -> bool {
        // Close each shard under its own lock. After this loop no shard
        // will accept a push.
        let mut was_open = false;
        for shard in self.shards.iter() {
            let mut synced = shard.synced.lock();
            was_open |= shard.shared.close(&mut synced);
        }

        // Publish the closed state for lock-free observers.
        self.is_closed.store(true, Release);

        was_open
    }

    /// Pushes a task into the queue.
    ///
    /// Selects a shard using the calling thread's home-shard index. Does
    /// nothing if the selected shard is closed (which implies all shards
    /// are closed, as `close` is the only path that sets the flag).
    pub(crate) fn push(&self, task: task::Notified<T>) {
        let idx = self.next_push_shard();
        let shard = &*self.shards[idx];

        let mut synced = shard.synced.lock();
        // safety: `synced` belongs to `shard.shared`
        unsafe { shard.shared.push(&mut synced, task) };
    }

    /// Pushes a batch of tasks. The whole batch is placed in a single
    /// shard to avoid taking multiple locks.
    pub(crate) fn push_batch<I>(&self, iter: I)
    where
        I: Iterator<Item = task::Notified<T>>,
    {
        let idx = self.next_push_shard();
        let shard = &*self.shards[idx];

        // safety: `&shard.synced` yields `&mut Synced` for the same
        // `Shared` instance that `push_batch` operates on. The underlying
        // implementation links the batch outside the lock and only
        // acquires it for the list splice.
        unsafe { shard.shared.push_batch(&shard.synced, iter) };
    }

    /// Pops a single task, rotating through shards starting at `hint`.
    pub(crate) fn pop(&self, hint: usize) -> Option<task::Notified<T>> {
        let num_shards = self.shards.len();
        let start = hint & self.shard_mask;

        for i in 0..num_shards {
            let idx = (start + i) & self.shard_mask;
            let shard = &*self.shards[idx];

            // Fast path: skip empty shards without locking.
            if shard.shared.is_empty() {
                continue;
            }

            let mut synced = shard.synced.lock();
            // safety: `synced` belongs to `shard.shared`
            if let Some(task) = unsafe { shard.shared.pop(&mut synced) } {
                return Some(task);
            }
        }

        None
    }

    /// Pops up to `n` tasks from the first non-empty shard, starting the
    /// search at `hint`, and passes them to `f`.
    ///
    /// Draining from a single shard keeps the critical section short and
    /// bounded; if that shard has fewer than `n` tasks, fewer are yielded.
    /// The caller will return for more on a subsequent tick.
    ///
    /// Returns `None` (without calling `f`) if no shard has any tasks.
    pub(crate) fn pop_n<R>(
        &self,
        hint: usize,
        n: usize,
        f: impl FnOnce(Pop<'_, T>) -> R,
    ) -> Option<R> {
        debug_assert!(n > 0);

        let num_shards = self.shards.len();
        let start = hint & self.shard_mask;

        for i in 0..num_shards {
            let idx = (start + i) & self.shard_mask;
            let shard = &*self.shards[idx];

            if shard.shared.is_empty() {
                continue;
            }

            let mut synced = shard.synced.lock();
            // Re-check under the lock: another worker may have drained
            // this shard between the atomic check and the lock.
            if shard.shared.is_empty() {
                continue;
            }

            // safety: `synced` belongs to `shard.shared`
            let pop = unsafe { shard.shared.pop_n(&mut synced, n) };
            return Some(f(pop));
        }

        None
    }

    /// Picks the shard for a push operation.
    ///
    /// Each thread is assigned a shard on first push and sticks with it.
    /// This keeps a single thread's pushes cache-local while spreading
    /// distinct threads across distinct mutexes.
    #[cfg(not(loom))]
    fn next_push_shard(&self) -> usize {
        // If there's only one shard, skip the thread-local lookup.
        if self.shard_mask == 0 {
            return 0;
        }

        PUSH_SHARD
            .try_with(|cell| {
                let mut idx = cell.get();
                if idx == UNASSIGNED {
                    idx = NEXT_SHARD.fetch_add(1, Relaxed);
                    cell.set(idx);
                }
                idx & self.shard_mask
            })
            .unwrap_or(0)
    }

    #[cfg(loom)]
    fn next_push_shard(&self) -> usize {
        // Shard count is capped at 1 under loom.
        debug_assert_eq!(self.shard_mask, 0);
        0
    }
}

// `Shared::push_batch` links the batch before acquiring the lock via the
// `Lock` trait. Implementing `Lock` on a shard's mutex reference lets us
// reuse that machinery, keeping the critical section to just the splice.
impl<'a> super::super::Lock<Synced> for &'a Mutex<Synced> {
    type Handle = MutexGuard<'a, Synced>;

    fn lock(self) -> Self::Handle {
        self.lock()
    }
}

impl AsMut<Synced> for MutexGuard<'_, Synced> {
    fn as_mut(&mut self) -> &mut Synced {
        self
    }
}
