use park::{BoxPark, BoxUnpark};
use task::Task;
use worker::state::{State, PUSHED_MASK};

use std::cell::UnsafeCell;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::atomic::Ordering::{AcqRel, Relaxed};

use crossbeam_utils::CachePadded;
use deque;

// TODO: None of the fields should be public
//
// It would also be helpful to split up the state across what fields /
// operations are thread-safe vs. which ones require ownership of the worker.
pub(crate) struct WorkerEntry {
    // Worker state. This is mutated when notifying the worker.
    //
    // The `usize` value is deserialized to a `worker::State` instance. See
    // comments on that type.
    pub state: CachePadded<AtomicUsize>,

    // Next entry in the parked Trieber stack
    next_sleeper: UnsafeCell<usize>,

    // Worker half of deque
    worker: deque::Worker<Arc<Task>>,

    // Stealer half of deque
    stealer: deque::Stealer<Arc<Task>>,

    // Thread parker
    pub park: UnsafeCell<BoxPark>,

    // Thread unparker
    pub unpark: BoxUnpark,
}

impl WorkerEntry {
    pub fn new(park: BoxPark, unpark: BoxUnpark) -> Self {
        let (w, s) = deque::fifo();

        WorkerEntry {
            state: CachePadded::new(AtomicUsize::new(State::default().into())),
            next_sleeper: UnsafeCell::new(0),
            worker: w,
            stealer: s,
            park: UnsafeCell::new(park),
            unpark,
        }
    }

    /// Atomically unset the pushed flag.
    ///
    /// # Return
    ///
    /// The state *before* the push flag is unset.
    ///
    /// # Ordering
    ///
    /// The specified ordering is established on the entry's state variable.
    pub fn fetch_unset_pushed(&self, ordering: Ordering) -> State {
        self.state.fetch_and(!PUSHED_MASK, ordering).into()
    }

    /// Submit a task to this worker while currently on the same thread that is
    /// running the worker.
    #[inline]
    pub fn submit_internal(&self, task: Arc<Task>) {
        self.push_internal(task);
    }

    /// Notifies the worker and returns `false` if it needs to be spawned.
    ///
    /// # Ordering
    ///
    /// The `state` must have been obtained with an `Acquire` ordering.
    #[inline]
    pub fn notify(&self, mut state: State) -> bool {
        use worker::Lifecycle::*;

        loop {
            let mut next = state;
            next.notify();

            let actual = self.state.compare_and_swap(
                state.into(), next.into(),
                AcqRel).into();

            if state == actual {
                break;
            }

            state = actual;
        }

        match state.lifecycle() {
            Sleeping => {
                // The worker is currently sleeping, the condition variable must
                // be signaled
                self.wakeup();
                true
            }
            Shutdown => false,
            Running | Notified | Signaled => {
                // In these states, the worker is active and will eventually see
                // the task that was just submitted.
                true
            }
        }
    }

    /// Signals to the worker that it should stop
    ///
    /// `state` is the last observed state for the worker. This allows skipping
    /// the initial load from the state atomic.
    ///
    /// # Return
    ///
    /// Returns `Ok` when the worker was successfully signaled.
    ///
    /// Returns `Err` if the worker has already terminated.
    pub fn signal_stop(&self, mut state: State) {
        use worker::Lifecycle::*;

        // Transition the worker state to signaled
        loop {
            let mut next = state;

            match state.lifecycle() {
                Shutdown => {
                    return;
                }
                Running | Sleeping => {}
                Notified | Signaled => {
                    // These two states imply that the worker is active, thus it
                    // will eventually see the shutdown signal, so we don't need
                    // to do anything.
                    //
                    // The worker is forced to see the shutdown signal
                    // eventually as:
                    //
                    // a) No more work will arrive
                    // b) The shutdown signal is stored as the head of the
                    // sleep, stack which will prevent the worker from going to
                    // sleep again.
                    return;
                }
            }

            next.set_lifecycle(Signaled);

            let actual = self.state.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if actual == state {
                break;
            }

            state = actual;
        }

        // Wakeup the worker
        self.wakeup();
    }

    /// Pop a task
    ///
    /// This **must** only be called by the thread that owns the worker entry.
    /// This function is not `Sync`.
    #[inline]
    pub fn pop_task(&self) -> deque::Pop<Arc<Task>> {
        self.worker.pop()
    }

    /// Steal tasks
    ///
    /// This is called by *other* workers to steal a task for processing. This
    /// function is `Sync`.
    ///
    /// At the same time, this method steals some additional tasks and moves
    /// them into `dest` in order to balance the work distribution among
    /// workers.
    pub fn steal_tasks(&self, dest: &Self) -> deque::Steal<Arc<Task>> {
        self.stealer.steal_many(&dest.worker)
    }

    /// Drain (and drop) all tasks that are queued for work.
    ///
    /// This is called when the pool is shutting down.
    pub fn drain_tasks(&self) {
        use deque::Pop::*;

        loop {
            match self.worker.pop() {
                Data(_) => {}
                Empty => break,
                Retry => {}
            }
        }
    }

    #[inline]
    pub fn push_internal(&self, task: Arc<Task>) {
        self.worker.push(task);
    }

    #[inline]
    pub fn wakeup(&self) {
        self.unpark.unpark();
    }

    #[inline]
    pub fn next_sleeper(&self) -> usize {
        unsafe { *self.next_sleeper.get() }
    }

    #[inline]
    pub fn set_next_sleeper(&self, val: usize) {
        unsafe { *self.next_sleeper.get() = val; }
    }
}

impl fmt::Debug for WorkerEntry {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("WorkerEntry")
            .field("state", &self.state.load(Relaxed))
            .field("next_sleeper", &"UnsafeCell<usize>")
            .field("worker", &self.worker)
            .field("stealer", &self.stealer)
            .field("park", &"UnsafeCell<BoxPark>")
            .field("unpark", &"BoxUnpark")
            .finish()
    }
}
