//! Coordinates idling workers

#![allow(dead_code)]

use crate::loom::sync::atomic::{AtomicBool, AtomicUsize};
use crate::loom::sync::MutexGuard;
use crate::runtime::scheduler::multi_thread_alt::{worker, Core, Handle, Shared};

use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};

pub(super) struct Idle {
    /// Number of searching cores
    num_searching: AtomicUsize,

    /// Number of idle cores
    num_idle: AtomicUsize,

    /// Map of idle cores
    idle_map: IdleMap,

    /// Used to catch false-negatives when waking workers
    needs_searching: AtomicBool,

    /// Total number of cores
    num_cores: usize,
}

pub(super) struct IdleMap {
    chunks: Vec<AtomicUsize>,
}

pub(super) struct Snapshot {
    chunks: Vec<usize>,
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
            idle_map: IdleMap::new(&cores),
            needs_searching: AtomicBool::new(false),
            num_cores: cores.len(),
        };

        let synced = Synced {
            sleepers: Vec::with_capacity(num_workers),
            available_cores: cores,
        };

        (idle, synced)
    }

    pub(super) fn needs_searching(&self) -> bool {
        self.needs_searching.load(Acquire)
    }

    pub(super) fn num_idle(&self, synced: &Synced) -> usize {
        #[cfg(not(loom))]
        debug_assert_eq!(synced.available_cores.len(), self.num_idle.load(Acquire));
        synced.available_cores.len()
    }

    pub(super) fn num_searching(&self) -> usize {
        self.num_searching.load(Acquire)
    }

    pub(super) fn snapshot(&self, snapshot: &mut Snapshot) {
        snapshot.update(&self.idle_map)
    }

    /// Try to acquire an available core
    pub(super) fn try_acquire_available_core(&self, synced: &mut Synced) -> Option<Box<Core>> {
        let ret = synced.available_cores.pop();

        if let Some(core) = &ret {
            // Decrement the number of idle cores
            let num_idle = self.num_idle.load(Acquire) - 1;
            debug_assert_eq!(num_idle, synced.available_cores.len());
            self.num_idle.store(num_idle, Release);

            self.idle_map.unset(core.index);
            debug_assert!(self.idle_map.matches(&synced.available_cores));
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

        if self.num_idle.load(Acquire) == 0 {
            self.needs_searching.store(true, Release);
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

        super::counters::inc_num_unparks_local();

        // Acquire the lock
        let synced = shared.synced.lock();
        self.notify_synced(synced, shared);
    }

    /// Notifies a single worker
    pub(super) fn notify_remote(&self, synced: MutexGuard<'_, worker::Synced>, shared: &Shared) {
        if synced.idle.sleepers.is_empty() {
            self.needs_searching.store(true, Release);
            return;
        }

        // We need to establish a stronger barrier than with `notify_local`
        self.num_searching.fetch_add(1, AcqRel);

        self.notify_synced(synced, shared);
    }

    /// Notify a worker while synced
    fn notify_synced(&self, mut synced: MutexGuard<'_, worker::Synced>, shared: &Shared) {
        // Find a sleeping worker
        if let Some(worker) = synced.idle.sleepers.pop() {
            // Find an available core
            if let Some(mut core) = self.try_acquire_available_core(&mut synced.idle) {
                debug_assert!(!core.is_searching);
                core.is_searching = true;

                // Assign the core to the worker
                synced.assigned_cores[worker] = Some(core);

                // Drop the lock before notifying the condvar.
                drop(synced);

                super::counters::inc_num_unparks_remote();

                // Notify the worker
                shared.condvars[worker].notify_one();
                return;
            } else {
                synced.idle.sleepers.push(worker);
            }
        }

        super::counters::inc_notify_no_core();

        // Set the `needs_searching` flag, this happens *while* the lock is held.
        self.needs_searching.store(true, Release);
        self.num_searching.fetch_sub(1, Release);

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
        debug_assert!(workers.is_empty());

        for _ in 0..num {
            if let Some(worker) = synced.idle.sleepers.pop() {
                // TODO: can this be switched to use next_available_core?
                if let Some(core) = synced.idle.available_cores.pop() {
                    debug_assert!(!core.is_searching);

                    self.idle_map.unset(core.index);

                    synced.assigned_cores[worker] = Some(core);

                    workers.push(worker);

                    continue;
                } else {
                    synced.idle.sleepers.push(worker);
                }
            }

            break;
        }

        if !workers.is_empty() {
            debug_assert!(self.idle_map.matches(&synced.idle.available_cores));
            let num_idle = synced.idle.available_cores.len();
            self.num_idle.store(num_idle, Release);
        } else {
            #[cfg(not(loom))]
            debug_assert_eq!(
                synced.idle.available_cores.len(),
                self.num_idle.load(Acquire)
            );
            self.needs_searching.store(true, Release);
        }
    }

    pub(super) fn shutdown(&self, synced: &mut worker::Synced, shared: &Shared) {
        // Wake every sleeping worker and assign a core to it. There may not be
        // enough sleeping workers for all cores, but other workers will
        // eventually find the cores and shut them down.
        while !synced.idle.sleepers.is_empty() && !synced.idle.available_cores.is_empty() {
            let worker = synced.idle.sleepers.pop().unwrap();
            let core = self.try_acquire_available_core(&mut synced.idle).unwrap();

            synced.assigned_cores[worker] = Some(core);
            shared.condvars[worker].notify_one();
        }

        debug_assert!(self.idle_map.matches(&synced.idle.available_cores));

        // Wake up any other workers
        while let Some(index) = synced.idle.sleepers.pop() {
            shared.condvars[index].notify_one();
        }
    }

    pub(super) fn shutdown_unassigned_cores(&self, handle: &Handle, shared: &Shared) {
        // If there are any remaining cores, shut them down here.
        //
        // This code is a bit convoluted to avoid lock-reentry.
        while let Some(core) = {
            let mut synced = shared.synced.lock();
            self.try_acquire_available_core(&mut synced.idle)
        } {
            shared.shutdown_core(handle, core);
        }
    }

    /// The worker releases the given core, making it available to other workers
    /// that are waiting.
    pub(super) fn release_core(&self, synced: &mut worker::Synced, core: Box<Core>) {
        // The core should not be searching at this point
        debug_assert!(!core.is_searching);

        // Check that there are no pending tasks in the global queue
        debug_assert!(synced.inject.is_empty());

        let num_idle = synced.idle.available_cores.len();
        #[cfg(not(loom))]
        debug_assert_eq!(num_idle, self.num_idle.load(Acquire));

        self.idle_map.set(core.index);

        // Store the core in the list of available cores
        synced.idle.available_cores.push(core);

        debug_assert!(self.idle_map.matches(&synced.idle.available_cores));

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

    pub(super) fn transition_worker_to_searching(&self, core: &mut Core) {
        core.is_searching = true;
        self.num_searching.fetch_add(1, AcqRel);
        self.needs_searching.store(false, Release);
    }

    /// A lightweight transition from searching -> running.
    ///
    /// Returns `true` if this is the final searching worker. The caller
    /// **must** notify a new worker.
    pub(super) fn transition_worker_from_searching(&self) -> bool {
        let prev = self.num_searching.fetch_sub(1, AcqRel);
        debug_assert!(prev > 0);

        prev == 1
    }
}

const BITS: usize = usize::BITS as usize;
const BIT_MASK: usize = (usize::BITS - 1) as usize;

impl IdleMap {
    fn new(cores: &[Box<Core>]) -> IdleMap {
        let ret = IdleMap::new_n(num_chunks(cores.len()));
        ret.set_all(cores);

        ret
    }

    fn new_n(n: usize) -> IdleMap {
        let chunks = (0..n).map(|_| AtomicUsize::new(0)).collect();
        IdleMap { chunks }
    }

    fn set(&self, index: usize) {
        let (chunk, mask) = index_to_mask(index);
        let prev = self.chunks[chunk].load(Acquire);
        let next = prev | mask;
        self.chunks[chunk].store(next, Release);
    }

    fn set_all(&self, cores: &[Box<Core>]) {
        for core in cores {
            self.set(core.index);
        }
    }

    fn unset(&self, index: usize) {
        let (chunk, mask) = index_to_mask(index);
        let prev = self.chunks[chunk].load(Acquire);
        let next = prev & !mask;
        self.chunks[chunk].store(next, Release);
    }

    fn matches(&self, idle_cores: &[Box<Core>]) -> bool {
        let expect = IdleMap::new_n(self.chunks.len());
        expect.set_all(idle_cores);

        for (i, chunk) in expect.chunks.iter().enumerate() {
            if chunk.load(Acquire) != self.chunks[i].load(Acquire) {
                return false;
            }
        }

        true
    }
}

impl Snapshot {
    pub(crate) fn new(idle: &Idle) -> Snapshot {
        let chunks = vec![0; idle.idle_map.chunks.len()];
        let mut ret = Snapshot { chunks };
        ret.update(&idle.idle_map);
        ret
    }

    fn update(&mut self, idle_map: &IdleMap) {
        for i in 0..self.chunks.len() {
            self.chunks[i] = idle_map.chunks[i].load(Acquire);
        }
    }

    pub(super) fn is_idle(&self, index: usize) -> bool {
        let (chunk, mask) = index_to_mask(index);
        debug_assert!(
            chunk < self.chunks.len(),
            "index={}; chunks={}",
            index,
            self.chunks.len()
        );
        self.chunks[chunk] & mask == mask
    }
}

fn num_chunks(max_cores: usize) -> usize {
    (max_cores / BITS) + 1
}

fn index_to_mask(index: usize) -> (usize, usize) {
    let mask = 1 << (index & BIT_MASK);
    let chunk = index / BITS;

    (chunk, mask)
}

fn num_active_workers(synced: &Synced) -> usize {
    synced.available_cores.capacity() - synced.available_cores.len()
}
