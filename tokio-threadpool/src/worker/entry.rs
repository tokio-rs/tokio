use park::{BoxPark, BoxUnpark};
use task::Task;
use worker::state::{State, PUSHED_MASK};

use std::cell::UnsafeCell;
use std::fmt;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_deque::{Steal, Stealer, Worker};
use crossbeam_queue::SegQueue;
use crossbeam_utils::CachePadded;
use slab::Slab;

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
    pub worker: Worker<Arc<Task>>,

    // Stealer half of deque
    stealer: Stealer<Arc<Task>>,

    // Thread parker
    park: UnsafeCell<Option<BoxPark>>,

    // Thread unparker
    unpark: UnsafeCell<Option<BoxUnpark>>,

    // Tasks that have been first polled by this worker, but not completed yet.
    running_tasks: UnsafeCell<Slab<Arc<Task>>>,

    // Tasks that have been first polled by this worker, but completed by another worker.
    remotely_completed_tasks: SegQueue<Arc<Task>>,

    // Set to `true` when `remotely_completed_tasks` has tasks that need to be removed from
    // `running_tasks`.
    needs_drain: AtomicBool,
}

impl WorkerEntry {
    pub fn new(park: BoxPark, unpark: BoxUnpark) -> Self {
        let w = Worker::new_fifo();
        let s = w.stealer();

        WorkerEntry {
            state: CachePadded::new(AtomicUsize::new(State::default().into())),
            next_sleeper: UnsafeCell::new(0),
            worker: w,
            stealer: s,
            park: UnsafeCell::new(Some(park)),
            unpark: UnsafeCell::new(Some(unpark)),
            running_tasks: UnsafeCell::new(Slab::new()),
            remotely_completed_tasks: SegQueue::new(),
            needs_drain: AtomicBool::new(false),
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

            let actual = self
                .state
                .compare_and_swap(state.into(), next.into(), AcqRel)
                .into();

            if state == actual {
                break;
            }

            state = actual;
        }

        match state.lifecycle() {
            Sleeping => {
                // The worker is currently sleeping, the condition variable must
                // be signaled
                self.unpark();
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

            let actual = self
                .state
                .compare_and_swap(state.into(), next.into(), AcqRel)
                .into();

            if actual == state {
                break;
            }

            state = actual;
        }

        // Wakeup the worker
        self.unpark();
    }

    /// Pop a task
    ///
    /// This **must** only be called by the thread that owns the worker entry.
    /// This function is not `Sync`.
    #[inline]
    pub fn pop_task(&self) -> Option<Arc<Task>> {
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
    pub fn steal_tasks(&self, dest: &Self) -> Steal<Arc<Task>> {
        self.stealer.steal_batch_and_pop(&dest.worker)
    }

    /// Drain (and drop) all tasks that are queued for work.
    ///
    /// This is called when the pool is shutting down.
    pub fn drain_tasks(&self) {
        while self.worker.pop().is_some() {}
    }

    /// Parks the worker thread.
    pub fn park(&self) {
        if let Some(park) = unsafe { (*self.park.get()).as_mut() } {
            park.park().unwrap();
        }
    }

    /// Parks the worker thread for at most `duration`.
    pub fn park_timeout(&self, duration: Duration) {
        if let Some(park) = unsafe { (*self.park.get()).as_mut() } {
            park.park_timeout(duration).unwrap();
        }
    }

    /// Unparks the worker thread.
    #[inline]
    pub fn unpark(&self) {
        if let Some(park) = unsafe { (*self.unpark.get()).as_ref() } {
            park.unpark();
        }
    }

    /// Registers a task in this worker.
    ///
    /// Called when the task is being polled for the first time.
    #[inline]
    pub fn register_task(&self, task: &Arc<Task>) {
        let running_tasks = unsafe { &mut *self.running_tasks.get() };

        let key = running_tasks.insert(task.clone());
        task.reg_index.set(key);
    }

    /// Unregisters a task from this worker.
    ///
    /// Called when the task is completed and was previously registered in this worker.
    #[inline]
    pub fn unregister_task(&self, task: Arc<Task>) {
        let running_tasks = unsafe { &mut *self.running_tasks.get() };
        running_tasks.remove(task.reg_index.get());
        self.drain_remotely_completed_tasks();
    }

    /// Unregisters a task from this worker.
    ///
    /// Called when the task is completed by another worker and was previously registered in this
    /// worker.
    #[inline]
    pub fn remotely_complete_task(&self, task: Arc<Task>) {
        self.remotely_completed_tasks.push(task);
        self.needs_drain.store(true, Release);
    }

    /// Drops the remaining incomplete tasks and the parker associated with this worker.
    ///
    /// This function is called by the shutdown trigger.
    pub fn shutdown(&self) {
        self.drain_remotely_completed_tasks();

        // Abort all incomplete tasks.
        let running_tasks = unsafe { &mut *self.running_tasks.get() };
        for (_, task) in running_tasks.iter() {
            task.abort();
        }
        running_tasks.clear();

        unsafe {
            *self.park.get() = None;
            *self.unpark.get() = None;
        }
    }

    /// Drains the `remotely_completed_tasks` queue and removes tasks from `running_tasks`.
    #[inline]
    fn drain_remotely_completed_tasks(&self) {
        if self.needs_drain.compare_and_swap(true, false, Acquire) {
            let running_tasks = unsafe { &mut *self.running_tasks.get() };

            while let Ok(task) = self.remotely_completed_tasks.pop() {
                running_tasks.remove(task.reg_index.get());
            }
        }
    }

    #[inline]
    pub fn push_internal(&self, task: Arc<Task>) {
        self.worker.push(task);
    }

    #[inline]
    pub fn next_sleeper(&self) -> usize {
        unsafe { *self.next_sleeper.get() }
    }

    #[inline]
    pub fn set_next_sleeper(&self, val: usize) {
        unsafe {
            *self.next_sleeper.get() = val;
        }
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
