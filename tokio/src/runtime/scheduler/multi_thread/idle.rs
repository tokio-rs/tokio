//! Coordinates idling workers

use crate::loom::sync::atomic::{AtomicBool, AtomicUsize};
use crate::loom::sync::MutexGuard;
use crate::runtime::scheduler::multi_thread::{worker, Core, Shared};

use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};

pub(super) struct Idle {
    /// Number of searching cores
    num_searching: AtomicUsize,

    /// Number of idle cores
    num_idle: AtomicUsize,

    /// Used to catch false-negatives when waking workers
    needs_searching: AtomicBool,

    /// Total number of cores
    num_cores: usize,
}

/// Data synchronized by the scheduler mutex
pub(super) struct Synced {
    /// Worker IDs that are currently sleeping
    sleepers: Vec<usize>,

    /// Cores available for workers
    available_cores: Vec<Box<Core>>,
}

impl Idle {
    pub(super) fn new(cores: Vec<Box<Core>>, num_workers: usize) -> (Idle, Synced) {
        let idle = Idle {
            num_searching: AtomicUsize::new(0),
            num_idle: AtomicUsize::new(cores.len()),
            needs_searching: AtomicBool::new(false),
            num_cores: cores.len(),
        };

        let synced = Synced {
            sleepers: Vec::with_capacity(num_workers),
            available_cores: cores,
        };

        (idle, synced)
    }

    pub(super) fn num_idle(&self, synced: &Synced) -> usize {
        debug_assert_eq!(synced.available_cores.len(), self.num_idle.load(Acquire));
        synced.available_cores.len()
    }

    /// Try to acquire an available core
    pub(super) fn try_acquire_available_core(&self, synced: &mut Synced) -> Option<Box<Core>> {
        let ret = synced.available_cores.pop();

        if ret.is_some() {
            // Decrement the number of idle cores
            let num_idle = self.num_idle.load(Acquire) - 1;
            debug_assert_eq!(num_idle, synced.available_cores.len());
            self.num_idle.store(num_idle, Release);
        }

        ret
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
            if let Some(mut core) = synced.idle.available_cores.pop() {
                debug_assert!(!core.is_searching);
                core.is_searching = is_searching;

                // Assign the core to the worker
                synced.assigned_cores[worker] = Some(core);

                let num_idle = synced.idle.available_cores.len();
                debug_assert_eq!(num_idle, self.num_idle.load(Acquire) - 1);

                // Update the number of sleeping workers
                self.num_idle.store(num_idle, Release);

                // Drop the lock before notifying the condvar.
                drop(synced);

                // Notify the worker
                shared.condvars[worker].notify_one();
                return;
            } else {
                // synced.idle.sleepers.push(worker);
                panic!("[tokio] unexpected condition");
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

    pub(super) fn notify_mult(
        &self,
        synced: &mut worker::Synced,
        workers: &mut Vec<usize>,
        num: usize,
    ) {
        let mut did_notify = false;

        for _ in 0..num {
            if let Some(worker) = synced.idle.sleepers.pop() {
                if let Some(core) = synced.idle.available_cores.pop() {
                    debug_assert!(!core.is_searching);

                    synced.assigned_cores[worker] = Some(core);

                    workers.push(worker);
                    did_notify = true;

                    continue;
                } else {
                    panic!("[tokio] unexpected condition");
                }
            }

            break;
        }

        if did_notify {
            let num_idle = synced.idle.available_cores.len();
            self.num_idle.store(num_idle, Release);
        } else {
            debug_assert_eq!(
                synced.idle.available_cores.len(),
                self.num_idle.load(Acquire)
            );
            self.needs_searching.store(true, Release);
        }
    }

    pub(super) fn shutdown(&self, synced: &mut worker::Synced, shared: &Shared) {
        // First, set the shutdown flag on each core
        for core in &mut synced.idle.available_cores {
            core.is_shutdown = true;
        }

        // Wake every sleeping worker and assign a core to it. There may not be
        // enough sleeping workers for all cores, but other workers will
        // eventually find the cores and shut them down.
        while !synced.idle.sleepers.is_empty() && !synced.idle.available_cores.is_empty() {
            let worker = synced.idle.sleepers.pop().unwrap();
            let core = synced.idle.available_cores.pop().unwrap();

            synced.assigned_cores[worker] = Some(core);
            shared.condvars[worker].notify_one();

            self.num_idle
                .store(synced.idle.available_cores.len(), Release);
        }

        // Wake up any other workers
        while let Some(index) = synced.idle.sleepers.pop() {
            shared.condvars[index].notify_one();
        }
    }

    /// The worker releases the given core, making it available to other workers
    /// that are waiting.
    pub(super) fn release_core(&self, synced: &mut worker::Synced, core: Box<Core>) {
        // The core should not be searching at this point
        debug_assert!(!core.is_searching);

        // Check that this isn't the final worker to go idle *and*
        // `needs_searching` is set.
        debug_assert!(!self.needs_searching.load(Acquire) || num_active_workers(&synced.idle) > 1);

        let num_idle = synced.idle.available_cores.len();
        debug_assert_eq!(num_idle, self.num_idle.load(Acquire));

        // Store the core in the list of available cores
        synced.idle.available_cores.push(core);

        // Update `num_idle`
        self.num_idle.store(num_idle + 1, Release);
    }

    pub(super) fn transition_worker_to_parked(&self, synced: &mut worker::Synced, index: usize) {
        // Store the worker index in the list of sleepers
        synced.idle.sleepers.push(index);

        // The worker's assigned core slot should be empty
        debug_assert!(synced.assigned_cores[index].is_none());
    }

    pub(super) fn try_transition_worker_to_searching(&self, core: &mut Core) {
        debug_assert!(!core.is_searching);

        let num_searching = self.num_searching.load(Acquire);
        let num_idle = self.num_idle.load(Acquire);

        if 2 * num_searching >= self.num_cores - num_idle {
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
        core.is_searching = false;

        let prev = self.num_searching.fetch_sub(1, AcqRel);
        debug_assert!(prev > 0);

        prev == 1
    }
}

fn num_active_workers(synced: &Synced) -> usize {
    synced.available_cores.capacity() - synced.available_cores.len()
}
