use park::{BoxPark, BoxUnpark};
use task::{Task, Queue};
use worker_state::{
    WorkerState,
    WORKER_SHUTDOWN,
    WORKER_SLEEPING,
};

use std::cell::UnsafeCell;
use std::fmt;
use std::sync::atomic::Ordering::{AcqRel, Relaxed};
use std::sync::atomic::AtomicUsize;

use deque;

pub(crate) struct WorkerEntry {
    // Worker state. This is mutated when notifying the worker.
    pub state: AtomicUsize,

    // Next entry in the parked Trieber stack
    next_sleeper: UnsafeCell<usize>,

    // Worker half of deque
    pub deque: deque::Deque<Task>,

    // Stealer half of deque
    pub steal: deque::Stealer<Task>,

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
            state: AtomicUsize::new(WorkerState::default().into()),
            next_sleeper: UnsafeCell::new(0),
            deque: w,
            steal: s,
            inbound: Queue::new(),
            park: UnsafeCell::new(park),
            unpark,
        }
    }

    #[inline]
    pub fn submit_internal(&self, task: Task) {
        self.push_internal(task);
    }

    /// Submits a task to the worker. This assumes that the caller is external
    /// to the worker. Internal submissions go through another path.
    ///
    /// Returns `false` if the worker needs to be spawned.
    pub fn submit_external(&self, task: Task, mut state: WorkerState) -> bool {
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
            WORKER_SLEEPING => {
                // The worker is currently sleeping, the condition variable must
                // be signaled
                self.wakeup();
                true
            }
            WORKER_SHUTDOWN => false,
            _ => true,
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
