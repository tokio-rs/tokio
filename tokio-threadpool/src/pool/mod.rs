mod backup;
mod backup_stack;
mod state;

pub(crate) use self::backup::{Backup, BackupId};
pub(crate) use self::backup_stack::MAX_BACKUP;
pub(crate) use self::state::{
    State,
    Lifecycle,
    MAX_FUTURES,
};

use self::backup::Handoff;
use self::backup_stack::BackupStack;

use config::Config;
use shutdown_task::ShutdownTask;
use task::{Task, Blocking};
use worker::{self, Worker, WorkerId};

use futures::Poll;
use futures::task::AtomicTask;

use std::cell::UnsafeCell;
use std::sync::atomic::Ordering::{Acquire, AcqRel, Relaxed};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::thread;

use rand::{Rng, SeedableRng, XorShiftRng};

// TODO: Rename this
#[derive(Debug)]
pub(crate) struct Pool {
    // ThreadPool state
    pub state: AtomicUsize,

    // Stack tracking sleeping workers.
    sleep_stack: worker::Stack,

    // Number of workers that haven't reached the final state of shutdown
    //
    // This is only used to know when to single `shutdown_task` once the
    // shutdown process has completed.
    pub num_workers: AtomicUsize,

    // Used to generate a thread local RNG seed
    pub next_thread_id: AtomicUsize,

    // Worker state
    //
    // A worker is a thread that is processing the work queue and polling
    // futures.
    //
    // This will *usually* be a small number.
    pub workers: Box<[worker::Entry]>,

    // Backup thread state
    //
    // In order to efficiently support `blocking`, a pool of backup threads is
    // needed. These backup threads are ready to take over a worker if the
    // future being processed requires blocking.
    backup: Box<[Backup]>,

    // Stack of sleeping backup threads
    pub backup_stack: BackupStack,

    // State regarding coordinating blocking sections and tracking tasks that
    // are pending blocking capacity.
    blocking: Blocking,

    // Task notified when the worker shuts down
    pub shutdown_task: ShutdownTask,

    // Configuration
    pub config: Config,
}

const TERMINATED: usize = 1;

impl Pool {
    /// Create a new `Pool`
    pub fn new(workers: Box<[worker::Entry]>, max_blocking: usize, config: Config) -> Pool {
        let pool_size = workers.len();
        let total_size = max_blocking + pool_size;

        // Create the set of backup entries
        //
        // This is `backup + pool_size` because the core thread pool running the
        // workers is spawned from backup as well.
        let backup = (0..total_size).map(|_| {
            Backup::new()
        }).collect::<Vec<_>>().into_boxed_slice();

        let backup_stack = BackupStack::new();

        for i in (0..backup.len()).rev() {
            backup_stack.push(&backup, BackupId(i))
                .unwrap();
        }

        // Initialize the blocking state
        let blocking = Blocking::new(max_blocking);

        let ret = Pool {
            state: AtomicUsize::new(State::new().into()),
            sleep_stack: worker::Stack::new(),
            num_workers: AtomicUsize::new(0),
            next_thread_id: AtomicUsize::new(0),
            workers,
            backup,
            backup_stack,
            blocking,
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

    pub fn is_shutdown(&self) -> bool {
        self.num_workers.load(Acquire) == TERMINATED
    }

    /// Called by `Worker` as it tries to enter a sleeping state. Before it
    /// sleeps, it must push itself onto the sleep stack. This enables other
    /// threads to see it when signaling work.
    pub fn push_sleeper(&self, idx: usize) -> Result<(), ()> {
        self.sleep_stack.push(&self.workers, idx)
    }

    pub fn terminate_sleeping_workers(&self) {
        use worker::Lifecycle::Signaled;

        // First, set the TERMINATED flag on `num_workers`. This signals that
        // whichever thread transitions the count to zero must notify the
        // shutdown task.
        let prev = self.num_workers.fetch_or(TERMINATED, AcqRel);
        let notify = prev == 0;

        trace!("  -> shutting down workers");
        // Wakeup all sleeping workers. They will wake up, see the state
        // transition, and terminate.
        while let Some((idx, worker_state)) = self.sleep_stack.pop(&self.workers, Signaled, true) {
            self.workers[idx].signal_stop(worker_state);
        }

        // Now terminate any backup threads
        //
        // The call to `pop` must be successful because shutting down the pool
        // is coordinated and at this point, this is the only thread that will
        // attempt to transition the backup stack to "terminated".
        while let Ok(Some(backup_id)) = self.backup_stack.pop(&self.backup, true) {
            self.backup[backup_id.0].signal_stop();
        }

        if notify {
            self.shutdown_task.notify();
        }
    }

    /// Track that a worker thread has started
    ///
    /// If `Err` is returned, then the thread is not permitted to started.
    fn thread_started(&self) -> Result<(), ()> {
        let mut curr = self.num_workers.load(Acquire);

        loop {
            if curr & TERMINATED == TERMINATED {
                return Err(());
            }

            let actual = self.num_workers.compare_and_swap(
                curr, curr + 2, AcqRel);

            if curr == actual {
                return Ok(());
            }

            curr = actual;
        }
    }

    fn thread_stopped(&self) {
        let prev = self.num_workers.fetch_sub(2, AcqRel);

        if prev == TERMINATED | 2 {
            self.shutdown_task.notify();
        }
    }

    pub fn poll_blocking_capacity(&self, task: &Arc<Task>) -> Poll<(), ::BlockingError> {
        self.blocking.poll_blocking_capacity(task)
    }

    /// Submit a task to the scheduler.
    ///
    /// Called from either inside or outside of the scheduler. If currently on
    /// the scheduler, then a fast path is taken.
    pub fn submit(&self, task: Arc<Task>, inner: &Arc<Pool>) {
        Worker::with_current(|worker| {
            match worker {
                // If the worker is in blocking mode, then even though the
                // thread-local variable is set, the current thread does not
                // have ownership of that worker entry. This is because the
                // worker entry has already been handed off to another thread.
                Some(worker) if !worker.is_blocking() => {
                    let idx = worker.id.0;

                    trace!("    -> submit internal; idx={}", idx);

                    worker.inner.workers[idx].submit_internal(task);
                    worker.inner.signal_work(inner);
                }
                _ => {
                    self.submit_external(task, inner);
                }
            }
        });
    }

    /// Submit a task to the scheduler from off worker
    ///
    /// Called from outside of the scheduler, this function is how new tasks
    /// enter the system.
    pub fn submit_external(&self, task: Arc<Task>, inner: &Arc<Pool>) {
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
            self.spawn_thread(WorkerId::new(idx), inner);
        }
    }

    pub fn release_backup(&self, backup_id: BackupId) -> Result<(), ()> {
        // First update the state, this cannot fail because the caller must have
        // exclusive access to the backup token.
        self.backup[backup_id.0].release();

        // Push the backup entry back on the stack
        self.backup_stack.push(&self.backup, backup_id)
    }

    pub fn notify_blocking_task(&self, pool: &Arc<Pool>) {
        self.blocking.notify_task(&pool);
    }

    /// Provision a thread to run a worker
    pub fn spawn_thread(&self, id: WorkerId, inner: &Arc<Pool>) {
        let backup_id = match self.backup_stack.pop(&self.backup, false) {
            Ok(Some(backup_id)) => backup_id,
            Ok(None) => panic!("no thread available"),
            Err(_) => {
                debug!("failed to spawn worker thread due to the thread pool shutting down");
                return;
            }
        };

        let need_spawn = self.backup[backup_id.0]
            .worker_handoff(id.clone());

        if !need_spawn {
            return;
        }

        if self.thread_started().is_err() {
            // The pool is shutting down.
            return;
        }

        let mut th = thread::Builder::new();

        if let Some(ref prefix) = inner.config.name_prefix {
            th = th.name(format!("{}{}", prefix, backup_id.0));
        }

        if let Some(stack) = inner.config.stack_size {
            th = th.stack_size(stack);
        }

        let inner = inner.clone();

        let res = th.spawn(move || {
            if let Some(ref f) = inner.config.after_start {
                f();
            }

            let mut worker_id = id;

            inner.backup[backup_id.0].start(&worker_id);

            loop {
                // The backup token should be in the running state.
                debug_assert!(inner.backup[backup_id.0].is_running());

                // TODO: Avoid always cloning
                let worker = Worker::new(worker_id, backup_id, inner.clone());

                // Run the worker. If the worker transitioned to a "blocking"
                // state, then `is_blocking` will be true.
                if !worker.do_run() {
                    // The worker shutdown, so exit the thread.
                    break;
                }

                // Push the thread back onto the backup stack. This makes it
                // available for future handoffs.
                //
                // This **must** happen before notifying the task.
                let res = inner.backup_stack
                    .push(&inner.backup, backup_id);

                if res.is_err() {
                    // The pool is being shutdown.
                    break;
                }

                // The task switched the current thread to blocking mode.
                // Now that the blocking task completed, any tasks
                inner.notify_blocking_task(&inner);

                debug_assert!(inner.backup[backup_id.0].is_running());

                // Wait for a handoff
                let handoff = inner.backup[backup_id.0]
                    .wait_for_handoff(true);

                match handoff {
                    Handoff::Worker(id) => {
                        debug_assert!(inner.backup[backup_id.0].is_running());
                        worker_id = id;
                    }
                    Handoff::Idle => {
                        // Worker is idle
                        break;
                    }
                    Handoff::Terminated => {
                        // TODO: When wait_for_handoff supports blocking with a
                        // timeout, this will have to be smarter
                        break;
                    }
                }
            }

            if let Some(ref f) = inner.config.before_stop {
                f();
            }

            inner.thread_stopped();
        });

        if let Err(e) = res {
            warn!("failed to spawn worker thread; err={:?}", e);
        }
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
                    self.spawn_thread(WorkerId(idx), inner);
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
