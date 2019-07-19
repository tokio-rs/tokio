mod backup;
mod backup_stack;
mod state;

pub(crate) use self::backup::{Backup, BackupId};
pub(crate) use self::backup_stack::MAX_BACKUP;
pub(crate) use self::state::{Lifecycle, State, MAX_FUTURES};

use self::backup::Handoff;
use self::backup_stack::BackupStack;

use config::Config;
use shutdown::ShutdownTrigger;
use task::{Blocking, Task};
use worker::{self, Worker, WorkerId};

use futures::Poll;

use std::cell::Cell;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::num::Wrapping;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{AcqRel, Acquire};
use std::sync::{Arc, Weak};
use std::thread;

use crossbeam_deque::Injector;
use crossbeam_utils::CachePadded;

#[derive(Debug)]
pub(crate) struct Pool {
    // Tracks the state of the thread pool (running, shutting down, ...).
    //
    // While workers check this field as a hint to detect shutdown, it is
    // **not** used as a primary point of coordination for workers. The sleep
    // stack is used as the primary point of coordination for workers.
    //
    // The value of this atomic is deserialized into a `pool::State` instance.
    // See comments for that type.
    pub state: CachePadded<AtomicUsize>,

    // Stack tracking sleeping workers.
    sleep_stack: CachePadded<worker::Stack>,

    // Worker state
    //
    // A worker is a thread that is processing the work queue and polling
    // futures.
    //
    // The number of workers will *usually* be small.
    pub workers: Arc<[worker::Entry]>,

    // The global MPMC queue of tasks.
    //
    // Spawned tasks are pushed into this queue. Although worker threads have their own dedicated
    // task queues, they periodically steal tasks from this global queue, too.
    pub queue: Arc<Injector<Arc<Task>>>,

    // Completes the shutdown process when the `ThreadPool` and all `Worker`s get dropped.
    //
    // When spawning a new `Worker`, this weak reference is upgraded and handed out to the new
    // thread.
    pub trigger: Weak<ShutdownTrigger>,

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

    // Configuration
    pub config: Config,
}

impl Pool {
    /// Create a new `Pool`
    pub fn new(
        workers: Arc<[worker::Entry]>,
        trigger: Weak<ShutdownTrigger>,
        max_blocking: usize,
        config: Config,
        queue: Arc<Injector<Arc<Task>>>,
    ) -> Pool {
        let pool_size = workers.len();
        let total_size = max_blocking + pool_size;

        // Create the set of backup entries
        //
        // This is `backup + pool_size` because the core thread pool running the
        // workers is spawned from backup as well.
        let backup = (0..total_size)
            .map(|_| Backup::new())
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let backup_stack = BackupStack::new();

        for i in (0..backup.len()).rev() {
            backup_stack.push(&backup, BackupId(i)).unwrap();
        }

        // Initialize the blocking state
        let blocking = Blocking::new(max_blocking);

        let ret = Pool {
            state: CachePadded::new(AtomicUsize::new(State::new().into())),
            sleep_stack: CachePadded::new(worker::Stack::new()),
            workers,
            queue,
            trigger,
            backup,
            backup_stack,
            blocking,
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

            let actual = self
                .state
                .compare_and_swap(state.into(), next.into(), AcqRel)
                .into();

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
    }

    pub fn poll_blocking_capacity(&self, task: &Arc<Task>) -> Poll<(), ::BlockingError> {
        self.blocking.poll_blocking_capacity(task)
    }

    /// Submit a task to the scheduler.
    ///
    /// Called from either inside or outside of the scheduler. If currently on
    /// the scheduler, then a fast path is taken.
    pub fn submit(&self, task: Arc<Task>, pool: &Arc<Pool>) {
        debug_assert_eq!(*self, **pool);

        Worker::with_current(|worker| {
            if let Some(worker) = worker {
                // If the worker is in blocking mode, then even though the
                // thread-local variable is set, the current thread does not
                // have ownership of that worker entry. This is because the
                // worker entry has already been handed off to another thread.
                //
                // The second check handles the case where the current thread is
                // part of a different threadpool than the one being submitted
                // to.
                if !worker.is_blocking() && *self == *worker.pool {
                    let idx = worker.id.0;

                    trace!("    -> submit internal; idx={}", idx);

                    worker.pool.workers[idx].submit_internal(task);
                    worker.pool.signal_work(pool);
                    return;
                }
            }

            self.submit_external(task, pool);
        });
    }

    /// Submit a task to the scheduler from off worker
    ///
    /// Called from outside of the scheduler, this function is how new tasks
    /// enter the system.
    pub fn submit_external(&self, task: Arc<Task>, pool: &Arc<Pool>) {
        debug_assert_eq!(*self, **pool);

        trace!("    -> submit external");

        self.queue.push(task);
        self.signal_work(pool);
    }

    pub fn release_backup(&self, backup_id: BackupId) -> Result<(), ()> {
        // First update the state, this cannot fail because the caller must have
        // exclusive access to the backup token.
        self.backup[backup_id.0].release();

        // Push the backup entry back on the stack
        self.backup_stack.push(&self.backup, backup_id)
    }

    pub fn notify_blocking_task(&self, pool: &Arc<Pool>) {
        debug_assert_eq!(*self, **pool);
        self.blocking.notify_task(&pool);
    }

    /// Provision a thread to run a worker
    pub fn spawn_thread(&self, id: WorkerId, pool: &Arc<Pool>) {
        debug_assert_eq!(*self, **pool);

        let backup_id = match self.backup_stack.pop(&self.backup, false) {
            Ok(Some(backup_id)) => backup_id,
            Ok(None) => panic!("no thread available"),
            Err(_) => {
                debug!("failed to spawn worker thread due to the thread pool shutting down");
                return;
            }
        };

        let need_spawn = self.backup[backup_id.0].worker_handoff(id.clone());

        if !need_spawn {
            return;
        }

        let trigger = match self.trigger.upgrade() {
            None => {
                // The pool is shutting down.
                return;
            }
            Some(t) => t,
        };

        let mut th = thread::Builder::new();

        if let Some(ref prefix) = pool.config.name_prefix {
            th = th.name(format!("{}{}", prefix, backup_id.0));
        }

        if let Some(stack) = pool.config.stack_size {
            th = th.stack_size(stack);
        }

        let pool = pool.clone();

        let res = th.spawn(move || {
            if let Some(ref f) = pool.config.after_start {
                f();
            }

            let mut worker_id = id;

            pool.backup[backup_id.0].start(&worker_id);

            loop {
                // The backup token should be in the running state.
                debug_assert!(pool.backup[backup_id.0].is_running());

                // TODO: Avoid always cloning
                let worker = Worker::new(worker_id, backup_id, pool.clone(), trigger.clone());

                // Run the worker. If the worker transitioned to a "blocking"
                // state, then `is_blocking` will be true.
                if !worker.do_run() {
                    // The worker shutdown, so exit the thread.
                    break;
                }

                debug_assert!(!pool.backup[backup_id.0].is_pushed());

                // Push the thread back onto the backup stack. This makes it
                // available for future handoffs.
                //
                // This **must** happen before notifying the task.
                let res = pool.backup_stack.push(&pool.backup, backup_id);

                if res.is_err() {
                    // The pool is being shutdown.
                    break;
                }

                // The task switched the current thread to blocking mode.
                // Now that the blocking task completed, any tasks
                pool.notify_blocking_task(&pool);

                debug_assert!(pool.backup[backup_id.0].is_running());

                // Wait for a handoff
                let handoff = pool.backup[backup_id.0].wait_for_handoff(pool.config.keep_alive);

                match handoff {
                    Handoff::Worker(id) => {
                        debug_assert!(pool.backup[backup_id.0].is_running());
                        worker_id = id;
                    }
                    Handoff::Idle | Handoff::Terminated => {
                        break;
                    }
                }
            }

            if let Some(ref f) = pool.config.before_stop {
                f();
            }
        });

        if let Err(e) = res {
            error!("failed to spawn worker thread; err={:?}", e);
            panic!("failed to spawn worker thread: {:?}", e);
        }
    }

    /// If there are any other workers currently relaxing, signal them that work
    /// is available so that they can try to find more work to process.
    pub fn signal_work(&self, pool: &Arc<Pool>) {
        debug_assert_eq!(*self, **pool);

        use worker::Lifecycle::Signaled;

        if let Some((idx, worker_state)) = self.sleep_stack.pop(&self.workers, Signaled, false) {
            let entry = &self.workers[idx];

            debug_assert!(
                worker_state.lifecycle() != Signaled,
                "actual={:?}",
                worker_state.lifecycle(),
            );

            trace!("signal_work -- notify; idx={}", idx);

            if !entry.notify(worker_state) {
                trace!("signal_work -- spawn; idx={}", idx);
                self.spawn_thread(WorkerId(idx), pool);
            }
        }
    }

    /// Generates a random number
    ///
    /// Uses a thread-local random number generator based on XorShift.
    pub fn rand_usize(&self) -> usize {
        thread_local! {
            static RNG: Cell<Wrapping<u32>> = Cell::new(Wrapping(prng_seed()));
        }

        RNG.with(|rng| {
            // This is the 32-bit variant of Xorshift.
            // https://en.wikipedia.org/wiki/Xorshift
            let mut x = rng.get();
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            rng.set(x);
            x.0 as usize
        })
    }
}

impl PartialEq for Pool {
    fn eq(&self, other: &Pool) -> bool {
        self as *const _ == other as *const _
    }
}

unsafe impl Send for Pool {}
unsafe impl Sync for Pool {}

// Return a thread-specific, 32-bit, non-zero seed value suitable for a 32-bit
// PRNG. This uses one libstd RandomState for a default hasher and hashes on
// the current thread ID to obtain an unpredictable, collision resistant seed.
fn prng_seed() -> u32 {
    // This obtains a small number of random bytes from the host system (for
    // example, on unix via getrandom(2)) in order to seed an unpredictable and
    // HashDoS resistant 64-bit hash function (currently: `SipHasher13` with
    // 128-bit state). We only need one of these, to make the seeds for all
    // process threads different via hashed IDs, collision resistant, and
    // unpredictable.
    lazy_static! {
        static ref RND_STATE: RandomState = RandomState::new();
    }

    // Hash the current thread ID to produce a u32 value
    let mut hasher = RND_STATE.build_hasher();
    thread::current().id().hash(&mut hasher);
    let hash: u64 = hasher.finish();
    let seed = (hash as u32) ^ ((hash >> 32) as u32);

    // Ensure non-zero seed (Xorshift yields only zero's for that seed)
    if seed == 0 {
        0x9b4e_6d25 // misc bits, could be any non-zero
    } else {
        seed
    }
}
