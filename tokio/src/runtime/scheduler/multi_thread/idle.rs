//! Coordinates idling workers

use crate::loom::sync::atomic::{AtomicBool, AtomicUsize};
use crate::loom::sync::MutexGuard;
use crate::runtime::scheduler::multi_thread::{worker, Core, Shared};

use std::fmt;
use std::sync::atomic::Ordering::{self, AcqRel, Acquire, Release};

pub(super) struct Idle {
    /// Number of searching workers
    num_searching: AtomicUsize,

    /// Number of sleeping workers
    num_sleeping: AtomicUsize,

    /// Used to catch false-negatives when waking workers
    needs_searching: AtomicBool,

    /// Total number of workers
    num_workers: usize,
}

/// Data synchronized by the scheduler mutex
pub(super) struct Synced {
    /// Worker IDs that are currently sleeping
    sleepers: Vec<usize>,
}

impl Idle {
    pub(super) fn new(num_workers: usize) -> (Idle, Synced) {
        let idle = Idle {
            num_searching: AtomicUsize::new(0),
            num_sleeping: AtomicUsize::new(0),
            needs_searching: AtomicBool::new(false),
            num_workers,
        };

        let synced = Synced {
            sleepers: Vec::with_capacity(num_workers),
        };

        (idle, synced)
    }

    /// We need at least one searching worker
    pub(super) fn notify_local(&self, shared: &Shared) {
        if self.num_searching.load(Acquire) != 0 {
            // There already is a searching worker. Note, that this could be a
            // false positive. However, because this method is called **from** a
            // worker, we know that there is at least one worker currently
            // awake, so the scheduler won't deadlock.
            return;
        }

        // There aren't any searching workers. Try to initialize one
        if self
            .num_searching
            .compare_exchange(0, 1, AcqRel, Acquire)
            .is_err()
        {
            // Failing the compare_exchange means another thread concurrently
            // launched a searching worker.
            return;
        }

        // Acquire the lock
        let synced = shared.synced.lock();
        self.notify_synced(synced, shared, true);
    }

    /// Notifies a single worker
    pub(super) fn notify_remote(&self, synced: MutexGuard<'_, worker::Synced>, shared: &Shared) {
        self.notify_synced(synced, shared, false);
    }

    /// Notify a worker while synced
    fn notify_synced(
        &self,
        mut synced: MutexGuard<'_, worker::Synced>,
        shared: &Shared,
        is_searching: bool,
    ) {
        // Find a sleeping worker
        if let Some(worker) = synced.idle.sleepers.pop() {
            // Find an available core
            if let Some(mut core) = synced.available_cores.pop() {
                debug_assert!(!core.is_searching);
                core.is_searching = is_searching;

                // Assign the core to the worker
                synced.assigned_cores[worker] = Some(core);

                let num_sleeping = self.num_sleeping.load(Acquire) - 1;
                debug_assert_eq!(num_sleeping, synced.idle.sleepers.len());

                // Update the number of sleeping workers
                self.num_sleeping.store(num_sleeping, Release);

                // Drop the lock before notifying the condvar.
                drop(synced);

                // Notify the worker
                shared.condvars[worker].notify_one();
                return;
            } else {
                synced.idle.sleepers.push(worker);
            }
        }

        // Set the `needs_searching` flag, this happens *while* the lock is held.
        self.needs_searching.store(true, Release);

        if is_searching {
            self.num_searching.fetch_sub(1, Release);
        }

        // Explicit mutex guard drop to show that holding the guard to this
        // point is significant. `needs_searching` and `num_searching` must be
        // updated in the critical section.
        drop(synced);
    }

    pub(super) fn transition_worker_to_parked(
        &self,
        synced: &mut worker::Synced,
        core: Box<Core>,
        index: usize,
    ) {
        // The core should not be searching at this point
        debug_assert!(!core.is_searching);

        // Check that this isn't the final worker to go idle *and*
        // `needs_searching` is set.
        debug_assert!(!self.needs_searching.load(Acquire) || synced.idle.num_active_workers() > 1);

        let num_sleeping = synced.idle.sleepers.len();
        debug_assert_eq!(num_sleeping, self.num_sleeping.load(Acquire));

        // Store the worker index in the list of sleepers
        synced.idle.sleepers.push(index);

        // Store the core in the list of available cores
        synced.available_cores.push(core);

        // The worker's assigned core slot should be empty
        debug_assert!(synced.assigned_cores[index].is_none());
    }

    pub(super) fn try_transition_worker_to_searching(&self, core: &mut Core) {
        debug_assert!(!core.is_searching);

        let num_searching = self.num_searching.load(Acquire);
        let num_sleeping = self.num_sleeping.load(Acquire);

        if 2 * num_searching >= self.num_workers - num_sleeping {
            return;
        }

        self.transition_worker_to_searching(core);
    }

    /// Needs to happen while synchronized in order to avoid races
    pub(super) fn transition_worker_to_searching_if_needed(
        &self,
        _synced: &mut Synced,
        core: &mut Core,
    ) -> bool {
        if self.needs_searching.load(Acquire) {
            // Needs to be called while holding the lock
            self.transition_worker_to_searching(core);
            true
        } else {
            false
        }
    }

    fn transition_worker_to_searching(&self, core: &mut Core) {
        core.is_searching = true;
        self.num_searching.fetch_add(1, AcqRel);
        self.needs_searching.store(false, Release);
    }

    /// A lightweight transition from searching -> running.
    ///
    /// Returns `true` if this is the final searching worker. The caller
    /// **must** notify a new worker.
    pub(super) fn transition_worker_from_searching(&self, core: &mut Core) -> bool {
        debug_assert!(core.is_searching);

        let prev = self.num_searching.fetch_sub(1, AcqRel);
        debug_assert!(prev > 0);

        if prev == 1 {
            false
        } else {
            core.is_searching = false;
            false
        }
    }
}

impl Synced {
    fn num_active_workers(&self) -> usize {
        self.sleepers.capacity() - self.sleepers.len()
    }
}
