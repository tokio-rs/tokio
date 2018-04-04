mod state;

pub(crate) use self::state::{
    // TODO: Rename `State`
    PoolState,
    SHUTDOWN_ON_IDLE,
    SHUTDOWN_NOW,
    MAX_FUTURES,
};

use config::{Config, MAX_WORKERS};
use sleep_stack::{
    SleepStack,
    EMPTY,
    TERMINATED,
};
use shutdown_task::ShutdownTask;
use task::Task;
use worker::{self, Worker, WorkerId, WorkerState, PUSHED_MASK};

use futures::task::AtomicTask;

use std::cell::UnsafeCell;
use std::sync::atomic::Ordering::{Acquire, AcqRel, Release, Relaxed};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use rand::{Rng, SeedableRng, XorShiftRng};

// TODO: Rename this
#[derive(Debug)]
pub(crate) struct Inner {
    // ThreadPool state
    pub state: AtomicUsize,

    // Stack tracking sleeping workers.
    pub sleep_stack: AtomicUsize,

    // Number of workers who haven't reached the final state of shutdown
    //
    // This is only used to know when to single `shutdown_task` once the
    // shutdown process has completed.
    pub num_workers: AtomicUsize,

    // Used to generate a thread local RNG seed
    pub next_thread_id: AtomicUsize,

    // Storage for workers
    //
    // This will *usually* be a small number
    pub workers: Box<[worker::Entry]>,

    // Task notified when the worker shuts down
    pub shutdown_task: ShutdownTask,

    // Configuration
    pub config: Config,
}

impl Inner {
    /// Create a new `Inner`
    pub fn new(workers: Box<[worker::Entry]>, config: Config) -> Inner {
        let pool_size = workers.len();

        let ret = Inner {
            state: AtomicUsize::new(PoolState::new().into()),
            sleep_stack: AtomicUsize::new(SleepStack::new().into()),
            num_workers: AtomicUsize::new(pool_size),
            next_thread_id: AtomicUsize::new(0),
            workers,
            shutdown_task: ShutdownTask {
                task1: AtomicTask::new(),
                #[cfg(feature = "unstable-futures")]
                task2: futures2::task::AtomicWaker::new(),
            },
            config,
        };

        // Now, we prime the sleeper stack
        for i in 0..pool_size {
            ret.push_sleeper(i).unwrap();
        }

        ret
    }

    /// Start shutting down the pool. This means that no new futures will be
    /// accepted.
    pub fn shutdown(&self, now: bool, purge_queue: bool) {
        let mut state: PoolState = self.state.load(Acquire).into();

        trace!("shutdown; state={:?}", state);

        // For now, this must be true
        debug_assert!(!purge_queue || now);

        // Start by setting the SHUTDOWN flag
        loop {
            let mut next = state;

            let num_futures = next.num_futures();

            if next.lifecycle() >= SHUTDOWN_NOW {
                // Already transitioned to shutting down state

                if !purge_queue || num_futures == 0 {
                    // Nothing more to do
                    return;
                }

                // The queue must be purged
                debug_assert!(purge_queue);
                next.clear_num_futures();
            } else {
                next.set_lifecycle(if now || num_futures == 0 {
                    // If already idle, always transition to shutdown now.
                    SHUTDOWN_NOW
                } else {
                    SHUTDOWN_ON_IDLE
                });

                if purge_queue {
                    next.clear_num_futures();
                }
            }

            let actual = self.state.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if state == actual {
                state = next;
                break;
            }

            state = actual;
        }

        trace!("  -> transitioned to shutdown");

        // Only transition to terminate if there are no futures currently on the
        // pool
        if state.num_futures() != 0 {
            return;
        }

        self.terminate_sleeping_workers();
    }

    pub fn terminate_sleeping_workers(&self) {
        use worker::Lifecycle::Signaled;

        trace!("  -> shutting down workers");
        // Wakeup all sleeping workers. They will wake up, see the state
        // transition, and terminate.
        while let Some((idx, worker_state)) = self.pop_sleeper(Signaled, TERMINATED) {
            trace!("  -> shutdown worker; idx={:?}; state={:?}", idx, worker_state);
            self.signal_stop(idx, worker_state);
        }
    }

    /// Signals to the worker that it should stop
    fn signal_stop(&self, idx: usize, mut state: WorkerState) {
        use worker::Lifecycle::*;

        let worker = &self.workers[idx];

        // Transition the worker state to signaled
        loop {
            let mut next = state;

            match state.lifecycle() {
                Shutdown => {
                    trace!("signal_stop -- WORKER_SHUTDOWN; idx={}", idx);
                    // If the worker is in the shutdown state, then it will never be
                    // started again.
                    self.worker_terminated();

                    return;
                }
                Running | Sleeping => {}
                Notified | Signaled => {
                    trace!("signal_stop -- skipping; idx={}; state={:?}", idx, state);
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

            let actual = worker.state.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if actual == state {
                break;
            }

            state = actual;
        }

        // Wakeup the worker
        worker.wakeup();
    }

    pub fn worker_terminated(&self) {
        let prev = self.num_workers.fetch_sub(1, AcqRel);

        trace!("worker_terminated; num_workers={}", prev - 1);

        if 1 == prev {
            trace!("notifying shutdown task");
            self.shutdown_task.notify();
        }
    }

    /// Submit a task to the scheduler.
    ///
    /// Called from either inside or outside of the scheduler. If currently on
    /// the scheduler, then a fast path is taken.
    pub fn submit(&self, task: Task, inner: &Arc<Inner>) {
        Worker::with_current(|worker| {
            match worker {
                Some(worker) => {
                    let idx = worker.id.idx;

                    trace!("    -> submit internal; idx={}", idx);

                    worker.inner.workers[idx].submit_internal(task);
                    worker.inner.signal_work(inner);
                }
                None => {
                    self.submit_external(task, inner);
                }
            }
        });
    }

    /// Submit a task to the scheduler from off worker
    ///
    /// Called from outside of the scheduler, this function is how new tasks
    /// enter the system.
    fn submit_external(&self, task: Task, inner: &Arc<Inner>) {
        use worker::Lifecycle::Notified;

        // First try to get a handle to a sleeping worker. This ensures that
        // sleeping tasks get woken up
        if let Some((idx, state)) = self.pop_sleeper(Notified, EMPTY) {
            trace!("submit to existing worker; idx={}; state={:?}", idx, state);
            self.submit_to_external(idx, task, state, inner);
            return;
        }

        // All workers are active, so pick a random worker and submit the
        // task to it.
        let len = self.workers.len();
        let idx = self.rand_usize() % len;

        trace!("  -> submitting to random; idx={}", idx);

        let state: WorkerState = self.workers[idx].state.load(Acquire).into();
        self.submit_to_external(idx, task, state, inner);
    }

    fn submit_to_external(&self,
                          idx: usize,
                          task: Task,
                          state: WorkerState,
                          inner: &Arc<Inner>)
    {
        let entry = &self.workers[idx];

        if !entry.submit_external(task, state) {
            self.spawn_worker(idx, inner);
        }
    }

    fn spawn_worker(&self, idx: usize, inner: &Arc<Inner>) {
        Worker::spawn(WorkerId::new(idx), inner);
    }

    /// If there are any other workers currently relaxing, signal them that work
    /// is available so that they can try to find more work to process.
    pub fn signal_work(&self, inner: &Arc<Inner>) {
        use worker::Lifecycle::*;

        if let Some((idx, mut state)) = self.pop_sleeper(Signaled, EMPTY) {
            let entry = &self.workers[idx];

            debug_assert!(state.lifecycle() != Signaled, "actual={:?}", state.lifecycle());

            // Transition the worker state to signaled
            loop {
                let mut next = state;

                // pop_sleeper should skip these
                next.set_lifecycle(Signaled);

                let actual = entry.state.compare_and_swap(
                    state.into(), next.into(), AcqRel).into();

                if actual == state {
                    break;
                }

                state = actual;
            }

            // The state has been transitioned to signal, now we need to wake up
            // the worker if necessary.
            match state.lifecycle() {
                Sleeping => {
                    trace!("signal_work -- wakeup; idx={}", idx);
                    self.workers[idx].wakeup();
                }
                Shutdown => {
                    trace!("signal_work -- spawn; idx={}", idx);
                    Worker::spawn(WorkerId::new(idx), inner);
                }
                Running | Notified | Signaled => {
                    // The workers are already active. No need to wake them up.
                }
            }
        }
    }

    /// Push a worker on the sleep stack
    ///
    /// Returns `Err` if the pool has been terminated
    pub fn push_sleeper(&self, idx: usize) -> Result<(), ()> {
        let mut state: SleepStack = self.sleep_stack.load(Acquire).into();

        debug_assert!(WorkerState::from(self.workers[idx].state.load(Relaxed)).is_pushed());

        loop {
            let mut next = state;

            let head = state.head();

            if head == TERMINATED {
                // The pool is terminated, cannot push the sleeper.
                return Err(());
            }

            self.workers[idx].set_next_sleeper(head);
            next.set_head(idx);

            let actual = self.sleep_stack.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if state == actual {
                return Ok(());
            }

            state = actual;
        }
    }

    /// Pop a worker from the sleep stack
    fn pop_sleeper(&self, max_lifecycle: worker::Lifecycle, terminal: usize)
        -> Option<(usize, WorkerState)>
    {
        debug_assert!(terminal == EMPTY || terminal == TERMINATED);

        let mut state: SleepStack = self.sleep_stack.load(Acquire).into();

        loop {
            let head = state.head();

            if head == EMPTY {
                let mut next = state;
                next.set_head(terminal);

                if next == state {
                    debug_assert!(terminal == EMPTY);
                    return None;
                }

                let actual = self.sleep_stack.compare_and_swap(
                    state.into(), next.into(), AcqRel).into();

                if actual != state {
                    state = actual;
                    continue;
                }

                return None;
            } else if head == TERMINATED {
                return None;
            }

            debug_assert!(head < MAX_WORKERS);

            let mut next = state;

            let next_head = self.workers[head].next_sleeper();

            // TERMINATED can never be set as the "next pointer" on a worker.
            debug_assert!(next_head != TERMINATED);

            if next_head == EMPTY {
                next.set_head(terminal);
            } else {
                next.set_head(next_head);
            }

            let actual = self.sleep_stack.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if actual == state {
                // The worker has been removed from the stack, so the pushed bit
                // can be unset. Release ordering is used to ensure that this
                // operation happens after actually popping the task.
                debug_assert_eq!(1, PUSHED_MASK);

                // Unset the PUSHED flag and get the current state.
                let state: WorkerState = self.workers[head].state
                    // TODO This should be fetch_and(!PUSHED_MASK)
                    .fetch_sub(PUSHED_MASK, Release).into();

                if state.lifecycle() >= max_lifecycle {
                    // If the worker has already been notified, then it is
                    // warming up to do more work. In this case, try to pop
                    // another thread that might be in a relaxed state.
                    continue;
                }

                return Some((head, state));
            }

            state = actual;
        }
    }

    /// Generates a random number
    ///
    /// Uses a thread-local seeded XorShift.
    pub fn rand_usize(&self) -> usize {
        // Use a thread-local random number generator. If the thread does not
        // have one yet, then seed a new one
        thread_local!(static THREAD_RNG_KEY: UnsafeCell<Option<XorShiftRng>> = UnsafeCell::new(None));

        THREAD_RNG_KEY.with(|t| {
            #[cfg(target_pointer_width = "32")]
            fn new_rng(thread_id: usize) -> XorShiftRng {
                XorShiftRng::from_seed([
                    thread_id as u32,
                    0x00000000,
                    0xa8a7d469,
                    0x97830e05])
            }

            #[cfg(target_pointer_width = "64")]
            fn new_rng(thread_id: usize) -> XorShiftRng {
                XorShiftRng::from_seed([
                    thread_id as u32,
                    (thread_id >> 32) as u32,
                    0xa8a7d469,
                    0x97830e05])
            }

            let thread_id = self.next_thread_id.fetch_add(1, Relaxed);
            let rng = unsafe { &mut *t.get() };

            if rng.is_none() {
                *rng = Some(new_rng(thread_id));
            }

            rng.as_mut().unwrap().next_u32() as usize
        })
    }
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}
