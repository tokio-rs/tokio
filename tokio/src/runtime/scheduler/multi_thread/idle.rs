//! Coordinates idling workers

use crate::loom::sync::atomic::{AtomicBool, AtomicUsize};
use crate::loom::sync::Mutex;

use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};

pub(super) struct Idle {
    /// Number of searching workers
    num_searching: AtomicUsize,

    /// Number of sleeping workers
    num_sleeping: AtomicUsize,

    /// Used to catch false-negatives when notifying workers.
    needs_searching: AtomicBool,

    /// Sleeping workers
    sleepers: Mutex<Vec<usize>>,

    /// Total number of workers.
    num_workers: usize,
}

#[derive(Copy, Clone)]
struct State(usize);

impl Idle {
    pub(super) fn new(num_workers: usize) -> Idle {
        Idle {
            num_searching: AtomicUsize::new(0),
            num_sleeping: AtomicUsize::new(0),
            needs_searching: AtomicBool::new(false),
            sleepers: Mutex::new(Vec::with_capacity(num_workers)),
            num_workers,
        }
    }

    /// If there are no workers actively searching, returns the index of a
    /// worker currently sleeping.
    pub(super) fn worker_to_notify_local(&self) -> Option<usize> {
        // Because this is only called from a worker thread, we can be more
        // relaxed here. If we get false-negatives, the caller will eventually
        // process the work.
        //
        // Before attempting the CAS (which is expensive), do a load which is cheap.
        if self.num_searching.load(Acquire) != 0 {
            return None;
        }

        if self
            .num_searching
            .compare_exchange(0, 1, Acquire, Acquire)
            .is_err()
        {
            return None;
        }

        // We will notify a worker.
        let mut sleepers = self.sleepers.lock();

        if let Some(ret) = sleepers.pop() {
            self.num_sleeping
                .store(self.num_sleeping.load(Acquire) - 1, Release);
            return Some(ret);
        }

        self.num_searching.fetch_sub(1, Release);
        None
    }

    pub(super) fn worker_to_notify_remote(&self) -> Option<usize> {
        // Because this function is called from *outside* the runtime, we need
        // to be more aggressive with our synchronization. We need to create a
        // barrier between pushing a task into the queue (done right before
        // calling this method) and ensuring there is at least one spinning
        // thread. A load is not sufficient, we must also write to create the
        // release relationship.
        if self.num_searching.fetch_add(0, AcqRel) != 0 {
            return None;
        }

        // We just created the release ordering and also noticed there are no
        // searchers. Try incrementing the number of searches.
        if self
            .num_searching
            .compare_exchange(0, 1, AcqRel, Acquire)
            .is_err()
        {
            // A worker started searching, because of the ordering set by
            // `fetch_add` we don't need to do anything else
            return None;
        }

        // We will notify a worker.
        let mut sleepers = self.sleepers.lock();

        if let Some(ret) = sleepers.pop() {
            self.num_sleeping
                .store(self.num_sleeping.load(Acquire) - 1, Release);
            return Some(ret);
        }

        self.num_searching.fetch_sub(1, Release);

        super::counters::inc_num_need_searchers();

        // We failed to find a worker to wake, we need to make sure we don't lose this wake.
        self.needs_searching.store(true, Release);
        None
    }

    /// Returns `false` if the worker must transition back to searching
    pub(super) fn transition_worker_to_parked(&self, worker: usize) -> bool {
        // Acquire the lock
        let mut sleepers = self.sleepers.lock();

        if self.needs_searching.load(Acquire) {
            return false;
        }

        // Track the sleeping worker
        sleepers.push(worker);
        self.num_sleeping
            .store(self.num_sleeping.load(Acquire) + 1, Release);

        true
    }

    /// Returns `true` if the worker has become searching
    pub(super) fn try_transition_worker_to_searching(&self) -> bool {
        let num_searching = self.num_searching.load(Acquire);
        let num_sleeping = self.num_sleeping.load(Acquire);

        if 2 * num_searching >= self.num_workers - num_sleeping {
            return false;
        }

        self.transition_worker_to_searching();
        true
    }

    pub(super) fn transition_worker_to_searching(&self) {
        // Because we are about to become a searching worker, we can
        // optimistically clear the need searching flag.
        self.needs_searching.store(false, Release);

        self.num_searching.fetch_add(1, AcqRel);
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

    /// Unpark a specific worker. This happens if tasks are submitted from
    /// within the worker's park routine.
    ///
    /// Returns `true` if the worker was parked before calling the method.
    pub(super) fn unpark_worker_by_id(&self, worker_id: usize) -> bool {
        let mut sleepers = self.sleepers.lock();

        for index in 0..sleepers.len() {
            if sleepers[index] == worker_id {
                self.num_sleeping
                    .store(self.num_sleeping.load(Acquire) - 1, Release);
                sleepers.swap_remove(index);

                return true;
            }
        }

        false
    }

    /// Returns `true` if `worker_id` is contained in the sleep set.
    pub(super) fn is_parked(&self, worker_id: usize) -> bool {
        let sleepers = self.sleepers.lock();
        sleepers.contains(&worker_id)
    }
}
