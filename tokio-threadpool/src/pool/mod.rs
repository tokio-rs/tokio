mod state;
mod stack;

pub(crate) use self::state::{
    State,
    Lifecycle,
    MAX_FUTURES,
};
use self::stack::SleepStack;

use config::Config;
use shutdown_task::ShutdownTask;
use task::Task;
use worker::{self, Worker, WorkerId};

use futures::task::AtomicTask;

use std::cell::UnsafeCell;
use std::sync::atomic::Ordering::{Acquire, AcqRel, Relaxed};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use rand::{Rng, SeedableRng, XorShiftRng};

// TODO: Rename this
#[derive(Debug)]
pub(crate) struct Pool {
    // ThreadPool state
    pub state: AtomicUsize,

    // Stack tracking sleeping workers.
    sleep_stack: SleepStack,

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

impl Pool {
    /// Create a new `Pool`
    pub fn new(workers: Box<[worker::Entry]>, config: Config) -> Pool {
        let pool_size = workers.len();

        let ret = Pool {
            state: AtomicUsize::new(State::new().into()),
            sleep_stack: SleepStack::new(),
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
            ret.sleep_stack.push(&ret.workers, i).unwrap();
        }

        ret
    }

    /// Start shutting down the pool. This means that no new futures will be
    /// accepted.
    pub fn shutdown(&self, now: bool, purge_queue: bool) {
        let mut state: State = self.state.load(Acquire).into();

        trace!("shutdown; state={:?}", state);

        // For now, this must be true
        debug_assert!(!purge_queue || now);

        // Start by setting the shutdown flag
        loop {
            let mut next = state;

            let num_futures = next.num_futures();

            if next.lifecycle() == Lifecycle::ShutdownNow {
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
                    Lifecycle::ShutdownNow
                } else {
                    Lifecycle::ShutdownOnIdle
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

    /// Called by `Worker` as it tries to enter a sleeping state. Before it
    /// sleeps, it must push itself onto the sleep stack. This enables other
    /// threads to see it when signaling work.
    pub fn push_sleeper(&self, idx: usize) -> Result<(), ()> {
        self.sleep_stack.push(&self.workers, idx)
    }

    pub fn terminate_sleeping_workers(&self) {
        use worker::Lifecycle::Signaled;

        trace!("  -> shutting down workers");
        // Wakeup all sleeping workers. They will wake up, see the state
        // transition, and terminate.
        while let Some((idx, worker_state)) = self.sleep_stack.pop(&self.workers, Signaled, true) {
            trace!("  -> shutdown worker; idx={:?}; state={:?}", idx, worker_state);

            if self.workers[idx].signal_stop(worker_state).is_err() {
                // The worker is already in the shutdown state, immediately
                // track that it has terminated as the worker will never work
                // again.
                self.worker_terminated();
            }
        }
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
    pub fn submit(&self, task: Arc<Task>, inner: &Arc<Pool>) {
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
    fn submit_external(&self, task: Arc<Task>, inner: &Arc<Pool>) {
        use worker::Lifecycle::Notified;

        // First try to get a handle to a sleeping worker. This ensures that
        // sleeping tasks get woken up
        if let Some((idx, worker_state)) = self.sleep_stack.pop(&self.workers, Notified, false) {
            trace!("submit to existing worker; idx={}; state={:?}", idx, worker_state);
            self.submit_to_external(idx, task, worker_state, inner);
            return;
        }

        // All workers are active, so pick a random worker and submit the
        // task to it.
        let len = self.workers.len();
        let idx = self.rand_usize() % len;

        trace!("  -> submitting to random; idx={}", idx);

        let state = self.workers[idx].load_state();
        self.submit_to_external(idx, task, state, inner);
    }

    fn submit_to_external(&self,
                          idx: usize,
                          task: Arc<Task>,
                          state: worker::State,
                          inner: &Arc<Pool>)
    {
        let entry = &self.workers[idx];

        if !entry.submit_external(task, state) {
            self.spawn_worker(idx, inner);
        }
    }

    fn spawn_worker(&self, idx: usize, inner: &Arc<Pool>) {
        Worker::spawn(WorkerId::new(idx), inner);
    }

    /// If there are any other workers currently relaxing, signal them that work
    /// is available so that they can try to find more work to process.
    pub fn signal_work(&self, inner: &Arc<Pool>) {
        use worker::Lifecycle::*;

        if let Some((idx, mut worker_state)) = self.sleep_stack.pop(&self.workers, Signaled, false) {
            let entry = &self.workers[idx];

            debug_assert!(worker_state.lifecycle() != Signaled, "actual={:?}", worker_state.lifecycle());

            // Transition the worker state to signaled
            loop {
                let mut next = worker_state;

                next.set_lifecycle(Signaled);

                let actual = entry.state.compare_and_swap(
                    worker_state.into(), next.into(), AcqRel).into();

                if actual == worker_state {
                    break;
                }

                worker_state = actual;
            }

            // The state has been transitioned to signal, now we need to wake up
            // the worker if necessary.
            match worker_state.lifecycle() {
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

unsafe impl Send for Pool {}
unsafe impl Sync for Pool {}
