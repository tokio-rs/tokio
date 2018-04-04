use park::{BoxPark, BoxUnpark};
use task::{Task, Queue};
use worker::state::{State, PUSHED_MASK};

use std::cell::UnsafeCell;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::atomic::Ordering::{Acquire, AcqRel, Relaxed};

use deque;

// TODO: None of the fields should be public
//
// It would also be helpful to split up the state across what fields /
// operations are thread-safe vs. which ones require ownership of the worker.
pub(crate) struct WorkerEntry {
    // Worker state. This is mutated when notifying the worker.
    pub state: AtomicUsize,

    // Next entry in the parked Trieber stack
    next_sleeper: UnsafeCell<usize>,

    // Worker half of deque
    deque: deque::Deque<Task>,

    // Stealer half of deque
    steal: deque::Stealer<Task>,

    // Thread parker
    pub park: UnsafeCell<BoxPark>,

    // Thread unparker
    pub unpark: BoxUnpark,

    // MPSC queue of jobs submitted to the worker from an external source.
    pub inbound: Queue,
}

impl WorkerEntry {
    pub fn new(park: BoxPark, unpark: BoxUnpark) -> Self {
        let w = deque::Deque::new();
        let s = w.stealer();

        WorkerEntry {
            state: AtomicUsize::new(State::default().into()),
            next_sleeper: UnsafeCell::new(0),
            deque: w,
            steal: s,
            inbound: Queue::new(),
            park: UnsafeCell::new(park),
            unpark,
        }
    }

    /// Atomically load the worker's state
    ///
    /// # Ordering
    ///
    /// An `Acquire` ordering is established on the entry's state variable.
    pub fn load_state(&self) -> State {
        self.state.load(Acquire).into()
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
    pub fn submit_internal(&self, task: Task) {
        self.push_internal(task);
    }

    /// Submits a task to the worker. This assumes that the caller is external
    /// to the worker. Internal submissions go through another path.
    ///
    /// Returns `false` if the worker needs to be spawned.
    ///
    /// # Ordering
    ///
    /// The `state` must have been obtained with an `Acquire` ordering.
    pub fn submit_external(&self, task: Task, mut state: State) -> bool {
        use worker::Lifecycle::*;

        // Push the task onto the external queue
        self.push_external(task);

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
    pub fn signal_stop(&self, mut state: State) -> Result<(), ()> {
        use worker::Lifecycle::*;

        // Transition the worker state to signaled
        loop {
            let mut next = state;

            match state.lifecycle() {
                Shutdown => {
                    return Err(());
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
                    return Ok(());
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

        Ok(())
    }

    /// Pop a task
    ///
    /// This **must** only be called by the thread that owns the worker entry.
    /// This function is not `Sync`.
    pub fn pop_task(&self) -> deque::Steal<Task> {
        self.deque.steal()
    }

    /// Steal a task
    ///
    /// This is called by *other* workers to steal a task for processing. This
    /// function is `Sync`.
    pub fn steal_task(&self) -> deque::Steal<Task> {
        self.steal.steal()
    }

    /// Drain (and drop) all tasks that are queued for work.
    ///
    /// This is called when the pool is shutting down.
    pub fn drain_tasks(&self) {
        while let Some(_) = self.deque.pop() {
        }
    }

    #[inline]
    fn push_external(&self, task: Task) {
        self.inbound.push(task);
    }

    #[inline]
    pub fn push_internal(&self, task: Task) {
        self.deque.push(task);
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
            .field("deque", &self.deque)
            .field("steal", &self.steal)
            .field("park", &"UnsafeCell<BoxPark>")
            .field("unpark", &"BoxUnpark")
            .field("inbound", &self.inbound)
            .finish()
    }
}
