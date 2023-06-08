//! Coordinates idling workers

use crate::loom::sync::atomic::{AtomicBool, AtomicUsize};
use crate::runtime::scheduler::multi_thread::Shared;

use std::fmt;
use std::sync::atomic::Ordering::{self, SeqCst};

pub(super) struct Idle {
    /// Number of searching workers
    num_searching: AtomicUsize,

    /// Number of sleeping workers
    num_sleeping: AtomicUsize,

    /// Used to catch false-negatives when waking workers
    needs_searching: AtomicBool,
}

/// Data synchronized by the scheduler mutex
pub(super) struct Synced {
    /// Sleeping workers
    sleepers: Vec<usize>,
}

impl Idle {
    pub(super) fn new(num_workers: usize) -> (Idle, Synced) {
        /*
        let init = State::new(num_workers);

        let idle = Idle {
            state: AtomicUsize::new(init.into()),
            num_workers,
        };

        let synced = Synced {
            sleepers: Vec::with_capacity(num_workers),
        };

        (idle, synced)
        */
        todo!()
    }

    /// If there are no workers actively searching, returns the index of a
    /// worker currently sleeping.
    pub(super) fn worker_to_notify(&self, shared: &Shared) -> Option<usize> {
        /*
        // If at least one worker is spinning, work being notified will
        // eventually be found. A searching thread will find **some** work and
        // notify another worker, eventually leading to our work being found.
        //
        // For this to happen, this load must happen before the thread
        // transitioning `num_searching` to zero. Acquire / Release does not
        // provide sufficient guarantees, so this load is done with `SeqCst` and
        // will pair with the `fetch_sub(1)` when transitioning out of
        // searching.
        if !self.notify_should_wakeup() {
            return None;
        }

        // Acquire the lock
        let mut lock = shared.synced.lock();

        // Check again, now that the lock is acquired
        if !self.notify_should_wakeup() {
            return None;
        }

        // A worker should be woken up, atomically increment the number of
        // searching workers as well as the number of unparked workers.
        State::unpark_one(&self.state, 1);

        // Get the worker to unpark
        let ret = lock.idle.sleepers.pop();
        debug_assert!(ret.is_some());

        ret
        */
        todo!()
    }

    /// Returns `true` if the worker needs to do a final check for submitted
    /// work.
    pub(super) fn transition_worker_to_parked(
        &self,
        synced: &mut Synced,
        is_searching: bool,
    ) -> bool {
        /*
        // Acquire the lock
        let mut lock = shared.synced.lock();

        // Decrement the number of unparked threads
        let ret = State::dec_num_unparked(&self.state, is_searching);

        // Track the sleeping worker
        lock.idle.sleepers.push(worker);

        ret
        */
        todo!()
    }

    pub(super) fn transition_worker_to_searching(&self) -> bool {
        /*
        let state = State::load(&self.state, SeqCst);
        if 2 * state.num_searching() >= self.num_workers {
            return false;
        }

        // It is possible for this routine to allow more than 50% of the workers
        // to search. That is OK. Limiting searchers is only an optimization to
        // prevent too much contention.
        State::inc_num_searching(&self.state, SeqCst);
        true
        */
        todo!()
    }

    /// A lightweight transition from searching -> running.
    ///
    /// Returns `true` if this is the final searching worker. The caller
    /// **must** notify a new worker.
    pub(super) fn transition_worker_from_searching(&self) -> bool {
        // State::dec_num_searching(&self.state)
        todo!()
    }

    /// Unpark a specific worker. This happens if tasks are submitted from
    /// within the worker's park routine.
    ///
    /// Returns `true` if the worker was parked before calling the method.
    pub(super) fn unpark_worker_by_id(&self, shared: &Shared, worker_id: usize) -> bool {
        /*
        let mut lock = shared.synced.lock();
        let sleepers = &mut lock.idle.sleepers;

        for index in 0..sleepers.len() {
            if sleepers[index] == worker_id {
                sleepers.swap_remove(index);

                // Update the state accordingly while the lock is held.
                State::unpark_one(&self.state, 0);

                return true;
            }
        }

        false
        */
        todo!()
    }

    /// Returns `true` if `worker_id` is contained in the sleep set.
    pub(super) fn is_parked(&self, shared: &Shared, worker_id: usize) -> bool {
        let lock = shared.synced.lock();
        lock.idle.sleepers.contains(&worker_id)
    }

/*
    fn notify_should_wakeup(&self) -> bool {
        let state = State(self.state.fetch_add(0, SeqCst));
        state.num_searching() == 0 && state.num_unparked() < self.num_workers
    }
    */
}

