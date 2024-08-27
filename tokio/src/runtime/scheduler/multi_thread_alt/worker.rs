//! A scheduler is initialized with a fixed number of workers. Each worker is
//! driven by a thread. Each worker has a "core" which contains data such as the
//! run queue and other state. When `block_in_place` is called, the worker's
//! "core" is handed off to a new thread allowing the scheduler to continue to
//! make progress while the originating thread blocks.
//!
//! # Shutdown
//!
//! Shutting down the runtime involves the following steps:
//!
//!  1. The Shared::close method is called. This closes the inject queue and
//!     `OwnedTasks` instance and wakes up all worker threads.
//!
//!  2. Each worker thread observes the close signal next time it runs
//!     Core::maintenance by checking whether the inject queue is closed.
//!     The `Core::is_shutdown` flag is set to true.
//!
//!  3. The worker thread calls `pre_shutdown` in parallel. Here, the worker
//!     will keep removing tasks from `OwnedTasks` until it is empty. No new
//!     tasks can be pushed to the `OwnedTasks` during or after this step as it
//!     was closed in step 1.
//!
//!  5. The workers call Shared::shutdown to enter the single-threaded phase of
//!     shutdown. These calls will push their core to `Shared::shutdown_cores`,
//!     and the last thread to push its core will finish the shutdown procedure.
//!
//!  6. The local run queue of each core is emptied, then the inject queue is
//!     emptied.
//!
//! At this point, shutdown has completed. It is not possible for any of the
//! collections to contain any tasks at this point, as each collection was
//! closed first, then emptied afterwards.
//!
//! ## Spawns during shutdown
//!
//! When spawning tasks during shutdown, there are two cases:
//!
//!  * The spawner observes the `OwnedTasks` being open, and the inject queue is
//!    closed.
//!  * The spawner observes the `OwnedTasks` being closed and doesn't check the
//!    inject queue.
//!
//! The first case can only happen if the `OwnedTasks::bind` call happens before
//! or during step 1 of shutdown. In this case, the runtime will clean up the
//! task in step 3 of shutdown.
//!
//! In the latter case, the task was not spawned and the task is immediately
//! cancelled by the spawner.
//!
//! The correctness of shutdown requires both the inject queue and `OwnedTasks`
//! collection to have a closed bit. With a close bit on only the inject queue,
//! spawning could run in to a situation where a task is successfully bound long
//! after the runtime has shut down. With a close bit on only the `OwnedTasks`,
//! the first spawning situation could result in the notification being pushed
//! to the inject queue after step 6 of shutdown, which would leave a task in
//! the inject queue indefinitely. This would be a ref-count cycle and a memory
//! leak.

use crate::loom::sync::{Arc, Condvar, Mutex, MutexGuard};
use crate::runtime;
use crate::runtime::driver::Driver;
use crate::runtime::scheduler::multi_thread_alt::{
    idle, queue, stats, Counters, Handle, Idle, Overflow, Stats, TraceStatus,
};
use crate::runtime::scheduler::{self, inject, Lock};
use crate::runtime::task::{OwnedTasks, TaskHarnessScheduleHooks};
use crate::runtime::{blocking, coop, driver, task, Config, SchedulerMetrics, WorkerMetrics};
use crate::runtime::{context, TaskHooks};
use crate::util::atomic_cell::AtomicCell;
use crate::util::rand::{FastRand, RngSeedGenerator};

use std::cell::{Cell, RefCell};
use std::task::Waker;
use std::time::Duration;
use std::{cmp, thread};

cfg_unstable_metrics! {
    mod metrics;
}

mod taskdump_mock;

/// A scheduler worker
///
/// Data is stack-allocated and never migrates threads
pub(super) struct Worker {
    /// Used to schedule bookkeeping tasks every so often.
    tick: u32,

    /// True if the scheduler is being shutdown
    pub(super) is_shutdown: bool,

    /// True if the scheduler is being traced
    is_traced: bool,

    /// Counter used to track when to poll from the local queue vs. the
    /// injection queue
    num_seq_local_queue_polls: u32,

    /// How often to check the global queue
    global_queue_interval: u32,

    /// Used to collect a list of workers to notify
    workers_to_notify: Vec<usize>,

    /// Snapshot of idle core list. This helps speedup stealing
    idle_snapshot: idle::Snapshot,

    stats: stats::Ephemeral,
}

/// Core data
///
/// Data is heap-allocated and migrates threads.
#[repr(align(128))]
pub(super) struct Core {
    /// Index holding this core's remote/shared state.
    pub(super) index: usize,

    lifo_slot: Option<Notified>,

    /// The worker-local run queue.
    run_queue: queue::Local<Arc<Handle>>,

    /// True if the worker is currently searching for more work. Searching
    /// involves attempting to steal from other workers.
    pub(super) is_searching: bool,

    /// Per-worker runtime stats
    stats: Stats,

    /// Fast random number generator.
    rand: FastRand,
}

/// State shared across all workers
pub(crate) struct Shared {
    /// Per-core remote state.
    remotes: Box<[Remote]>,

    /// Global task queue used for:
    ///  1. Submit work to the scheduler while **not** currently on a worker thread.
    ///  2. Submit work to the scheduler when a worker run queue is saturated
    pub(super) inject: inject::Shared<Arc<Handle>>,

    /// Coordinates idle workers
    idle: Idle,

    /// Collection of all active tasks spawned onto this executor.
    pub(super) owned: OwnedTasks<Arc<Handle>>,

    /// Data synchronized by the scheduler mutex
    pub(super) synced: Mutex<Synced>,

    /// Power's Tokio's I/O, timers, etc... the responsibility of polling the
    /// driver is shared across workers.
    driver: AtomicCell<Driver>,

    /// Condition variables used to unblock worker threads. Each worker thread
    /// has its own `condvar` it waits on.
    pub(super) condvars: Vec<Condvar>,

    /// The number of cores that have observed the trace signal.
    pub(super) trace_status: TraceStatus,

    /// Scheduler configuration options
    config: Config,

    /// Collects metrics from the runtime.
    pub(super) scheduler_metrics: SchedulerMetrics,

    pub(super) worker_metrics: Box<[WorkerMetrics]>,

    /// Only held to trigger some code on drop. This is used to get internal
    /// runtime metrics that can be useful when doing performance
    /// investigations. This does nothing (empty struct, no drop impl) unless
    /// the `tokio_internal_mt_counters` `cfg` flag is set.
    _counters: Counters,
}

/// Data synchronized by the scheduler mutex
pub(crate) struct Synced {
    /// When worker is notified, it is assigned a core. The core is placed here
    /// until the worker wakes up to take it.
    pub(super) assigned_cores: Vec<Option<Box<Core>>>,

    /// Cores that have observed the shutdown signal
    ///
    /// The core is **not** placed back in the worker to avoid it from being
    /// stolen by a thread that was spawned as part of `block_in_place`.
    shutdown_cores: Vec<Box<Core>>,

    /// The driver goes here when shutting down
    shutdown_driver: Option<Box<Driver>>,

    /// Synchronized state for `Idle`.
    pub(super) idle: idle::Synced,

    /// Synchronized state for `Inject`.
    pub(crate) inject: inject::Synced,
}

/// Used to communicate with a worker from other threads.
struct Remote {
    /// When a task is scheduled from a worker, it is stored in this slot. The
    /// worker will check this slot for a task **before** checking the run
    /// queue. This effectively results in the **last** scheduled task to be run
    /// next (LIFO). This is an optimization for improving locality which
    /// benefits message passing patterns and helps to reduce latency.
    // lifo_slot: Lifo,

    /// Steals tasks from this worker.
    pub(super) steal: queue::Steal<Arc<Handle>>,
}

/// Thread-local context
pub(crate) struct Context {
    // Current scheduler's handle
    handle: Arc<Handle>,

    /// Worker index
    index: usize,

    /// True when the LIFO slot is enabled
    lifo_enabled: Cell<bool>,

    /// Core data
    core: RefCell<Option<Box<Core>>>,

    /// Used to pass cores to other threads when `block_in_place` is called
    handoff_core: Arc<AtomicCell<Core>>,

    /// Tasks to wake after resource drivers are polled. This is mostly to
    /// handle yielded tasks.
    pub(crate) defer: RefCell<Vec<Notified>>,
}

/// Running a task may consume the core. If the core is still available when
/// running the task completes, it is returned. Otherwise, the worker will need
/// to stop processing.
type RunResult = Result<Box<Core>, ()>;
type NextTaskResult = Result<(Option<Notified>, Box<Core>), ()>;

/// A task handle
type Task = task::Task<Arc<Handle>>;

/// A notified task handle
type Notified = task::Notified<Arc<Handle>>;

/// Value picked out of thin-air. Running the LIFO slot a handful of times
/// seems sufficient to benefit from locality. More than 3 times probably is
/// overweighing. The value can be tuned in the future with data that shows
/// improvements.
const MAX_LIFO_POLLS_PER_TICK: usize = 3;

pub(super) fn create(
    num_cores: usize,
    driver: Driver,
    driver_handle: driver::Handle,
    blocking_spawner: blocking::Spawner,
    seed_generator: RngSeedGenerator,
    config: Config,
) -> runtime::Handle {
    let mut num_workers = num_cores;

    // If the driver is enabled, we need an extra thread to handle polling the
    // driver when all cores are busy.
    if driver.is_enabled() {
        num_workers += 1;
    }

    let mut cores = Vec::with_capacity(num_cores);
    let mut remotes = Vec::with_capacity(num_cores);
    // Worker metrics are actually core based
    let mut worker_metrics = Vec::with_capacity(num_cores);

    // Create the local queues
    for i in 0..num_cores {
        let (steal, run_queue) = queue::local(config.local_queue_capacity);

        let metrics = WorkerMetrics::from_config(&config);
        let stats = Stats::new(&metrics);

        cores.push(Box::new(Core {
            index: i,
            lifo_slot: None,
            run_queue,
            is_searching: false,
            stats,
            rand: FastRand::from_seed(config.seed_generator.next_seed()),
        }));

        remotes.push(Remote {
            steal,
            // lifo_slot: Lifo::new(),
        });
        worker_metrics.push(metrics);
    }

    // Allocate num-cores + 1 workers, so one worker can handle the I/O driver,
    // if needed.
    let (idle, idle_synced) = Idle::new(cores, num_workers);
    let (inject, inject_synced) = inject::Shared::new();

    let handle = Arc::new(Handle {
        task_hooks: TaskHooks {
            task_spawn_callback: config.before_spawn.clone(),
            task_terminate_callback: config.after_termination.clone(),
        },
        shared: Shared {
            remotes: remotes.into_boxed_slice(),
            inject,
            idle,
            owned: OwnedTasks::new(num_cores),
            synced: Mutex::new(Synced {
                assigned_cores: (0..num_workers).map(|_| None).collect(),
                shutdown_cores: Vec::with_capacity(num_cores),
                shutdown_driver: None,
                idle: idle_synced,
                inject: inject_synced,
            }),
            driver: AtomicCell::new(Some(Box::new(driver))),
            condvars: (0..num_workers).map(|_| Condvar::new()).collect(),
            trace_status: TraceStatus::new(num_cores),
            config,
            scheduler_metrics: SchedulerMetrics::new(),
            worker_metrics: worker_metrics.into_boxed_slice(),
            _counters: Counters,
        },
        driver: driver_handle,
        blocking_spawner,
        seed_generator,
    });

    let rt_handle = runtime::Handle {
        inner: scheduler::Handle::MultiThreadAlt(handle),
    };

    // Eagerly start worker threads
    for index in 0..num_workers {
        let handle = rt_handle.inner.expect_multi_thread_alt();
        let h2 = handle.clone();
        let handoff_core = Arc::new(AtomicCell::new(None));

        handle
            .blocking_spawner
            .spawn_blocking(&rt_handle, move || run(index, h2, handoff_core, false));
    }

    rt_handle
}

#[track_caller]
pub(crate) fn block_in_place<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    // Try to steal the worker core back
    struct Reset(coop::Budget);

    impl Drop for Reset {
        fn drop(&mut self) {
            with_current(|maybe_cx| {
                if let Some(cx) = maybe_cx {
                    let core = cx.handoff_core.take();
                    let mut cx_core = cx.core.borrow_mut();
                    assert!(cx_core.is_none());
                    *cx_core = core;

                    // Reset the task budget as we are re-entering the
                    // runtime.
                    coop::set(self.0);
                }
            });
        }
    }

    let mut had_entered = false;

    let setup_result = with_current(|maybe_cx| {
        match (
            crate::runtime::context::current_enter_context(),
            maybe_cx.is_some(),
        ) {
            (context::EnterRuntime::Entered { .. }, true) => {
                // We are on a thread pool runtime thread, so we just need to
                // set up blocking.
                had_entered = true;
            }
            (
                context::EnterRuntime::Entered {
                    allow_block_in_place,
                },
                false,
            ) => {
                // We are on an executor, but _not_ on the thread pool.  That is
                // _only_ okay if we are in a thread pool runtime's block_on
                // method:
                if allow_block_in_place {
                    had_entered = true;
                    return Ok(());
                } else {
                    // This probably means we are on the current_thread runtime or in a
                    // LocalSet, where it is _not_ okay to block.
                    return Err(
                        "can call blocking only when running on the multi-threaded runtime",
                    );
                }
            }
            (context::EnterRuntime::NotEntered, true) => {
                // This is a nested call to block_in_place (we already exited).
                // All the necessary setup has already been done.
                return Ok(());
            }
            (context::EnterRuntime::NotEntered, false) => {
                // We are outside of the tokio runtime, so blocking is fine.
                // We can also skip all of the thread pool blocking setup steps.
                return Ok(());
            }
        }

        let cx = maybe_cx.expect("no .is_some() == false cases above should lead here");

        // Get the worker core. If none is set, then blocking is fine!
        let core = match cx.core.borrow_mut().take() {
            Some(core) => core,
            None => return Ok(()),
        };

        // In order to block, the core must be sent to another thread for
        // execution.
        //
        // First, move the core back into the worker's shared core slot.
        cx.handoff_core.set(core);

        // Next, clone the worker handle and send it to a new thread for
        // processing.
        //
        // Once the blocking task is done executing, we will attempt to
        // steal the core back.
        let index = cx.index;
        let handle = cx.handle.clone();
        let handoff_core = cx.handoff_core.clone();
        runtime::spawn_blocking(move || run(index, handle, handoff_core, true));
        Ok(())
    });

    if let Err(panic_message) = setup_result {
        panic!("{}", panic_message);
    }

    if had_entered {
        // Unset the current task's budget. Blocking sections are not
        // constrained by task budgets.
        let _reset = Reset(coop::stop());

        crate::runtime::context::exit_runtime(f)
    } else {
        f()
    }
}

fn run(
    index: usize,
    handle: Arc<Handle>,
    handoff_core: Arc<AtomicCell<Core>>,
    blocking_in_place: bool,
) {
    struct AbortOnPanic;

    impl Drop for AbortOnPanic {
        fn drop(&mut self) {
            if std::thread::panicking() {
                eprintln!("worker thread panicking; aborting process");
                std::process::abort();
            }
        }
    }

    // Catching panics on worker threads in tests is quite tricky. Instead, when
    // debug assertions are enabled, we just abort the process.
    #[cfg(debug_assertions)]
    let _abort_on_panic = AbortOnPanic;

    let num_workers = handle.shared.condvars.len();

    let mut worker = Worker {
        tick: 0,
        num_seq_local_queue_polls: 0,
        global_queue_interval: Stats::DEFAULT_GLOBAL_QUEUE_INTERVAL,
        is_shutdown: false,
        is_traced: false,
        workers_to_notify: Vec::with_capacity(num_workers - 1),
        idle_snapshot: idle::Snapshot::new(&handle.shared.idle),
        stats: stats::Ephemeral::new(),
    };

    let sched_handle = scheduler::Handle::MultiThreadAlt(handle.clone());

    crate::runtime::context::enter_runtime(&sched_handle, true, |_| {
        // Set the worker context.
        let cx = scheduler::Context::MultiThreadAlt(Context {
            index,
            lifo_enabled: Cell::new(!handle.shared.config.disable_lifo_slot),
            handle,
            core: RefCell::new(None),
            handoff_core,
            defer: RefCell::new(Vec::with_capacity(64)),
        });

        context::set_scheduler(&cx, || {
            let cx = cx.expect_multi_thread_alt();

            // Run the worker
            let res = worker.run(&cx, blocking_in_place);
            // `err` here signifies the core was lost, this is an expected end
            // state for a worker.
            debug_assert!(res.is_err());

            // Check if there are any deferred tasks to notify. This can happen when
            // the worker core is lost due to `block_in_place()` being called from
            // within the task.
            if !cx.defer.borrow().is_empty() {
                worker.schedule_deferred_without_core(&cx, &mut cx.shared().synced.lock());
            }
        });
    });
}

macro_rules! try_task {
    ($e:expr) => {{
        let (task, core) = $e?;
        if task.is_some() {
            return Ok((task, core));
        }
        core
    }};
}

macro_rules! try_task_new_batch {
    ($w:expr, $e:expr) => {{
        let (task, mut core) = $e?;
        if task.is_some() {
            core.stats.start_processing_scheduled_tasks(&mut $w.stats);
            return Ok((task, core));
        }
        core
    }};
}

impl Worker {
    fn run(&mut self, cx: &Context, blocking_in_place: bool) -> RunResult {
        let (maybe_task, mut core) = {
            if blocking_in_place {
                if let Some(core) = cx.handoff_core.take() {
                    (None, core)
                } else {
                    // Just shutdown
                    return Err(());
                }
            } else {
                let mut synced = cx.shared().synced.lock();

                // First try to acquire an available core
                if let Some(core) = self.try_acquire_available_core(cx, &mut synced) {
                    // Try to poll a task from the global queue
                    let maybe_task = cx.shared().next_remote_task_synced(&mut synced);
                    (maybe_task, core)
                } else {
                    // block the thread to wait for a core to be assigned to us
                    self.wait_for_core(cx, synced)?
                }
            }
        };

        cx.shared().worker_metrics[core.index].set_thread_id(thread::current().id());
        core.stats.start_processing_scheduled_tasks(&mut self.stats);

        if let Some(task) = maybe_task {
            core = self.run_task(cx, core, task)?;
        }

        while !self.is_shutdown {
            let (maybe_task, c) = self.next_task(cx, core)?;
            core = c;

            if let Some(task) = maybe_task {
                core = self.run_task(cx, core, task)?;
            } else {
                // The only reason to get `None` from `next_task` is we have
                // entered the shutdown phase.
                assert!(self.is_shutdown);
                break;
            }
        }

        cx.shared().shutdown_core(&cx.handle, core);

        // It is possible that tasks wake others during drop, so we need to
        // clear the defer list.
        self.shutdown_clear_defer(cx);

        Err(())
    }

    // Try to acquire an available core, but do not block the thread
    fn try_acquire_available_core(
        &mut self,
        cx: &Context,
        synced: &mut Synced,
    ) -> Option<Box<Core>> {
        if let Some(mut core) = cx
            .shared()
            .idle
            .try_acquire_available_core(&mut synced.idle)
        {
            self.reset_acquired_core(cx, synced, &mut core);
            Some(core)
        } else {
            None
        }
    }

    // Block the current thread, waiting for an available core
    fn wait_for_core(
        &mut self,
        cx: &Context,
        mut synced: MutexGuard<'_, Synced>,
    ) -> NextTaskResult {
        if cx.shared().idle.needs_searching() {
            if let Some(mut core) = self.try_acquire_available_core(cx, &mut synced) {
                cx.shared().idle.transition_worker_to_searching(&mut core);
                return Ok((None, core));
            }
        }

        cx.shared()
            .idle
            .transition_worker_to_parked(&mut synced, cx.index);

        // Wait until a core is available, then exit the loop.
        let mut core = loop {
            if let Some(core) = synced.assigned_cores[cx.index].take() {
                break core;
            }

            // If shutting down, abort
            if cx.shared().inject.is_closed(&synced.inject) {
                self.shutdown_clear_defer(cx);
                return Err(());
            }

            synced = cx.shared().condvars[cx.index].wait(synced).unwrap();
        };

        self.reset_acquired_core(cx, &mut synced, &mut core);

        if self.is_shutdown {
            // Currently shutting down, don't do any more work
            return Ok((None, core));
        }

        let n = cmp::max(core.run_queue.remaining_slots() / 2, 1);
        let maybe_task = self.next_remote_task_batch_synced(cx, &mut synced, &mut core, n);

        core.stats.unparked();
        self.flush_metrics(cx, &mut core);

        Ok((maybe_task, core))
    }

    /// Ensure core's state is set correctly for the worker to start using.
    fn reset_acquired_core(&mut self, cx: &Context, synced: &mut Synced, core: &mut Core) {
        self.global_queue_interval = core.stats.tuned_global_queue_interval(&cx.shared().config);

        // Reset `lifo_enabled` here in case the core was previously stolen from
        // a task that had the LIFO slot disabled.
        self.reset_lifo_enabled(cx);

        // At this point, the local queue should be empty
        #[cfg(not(loom))]
        debug_assert!(core.run_queue.is_empty());

        // Update shutdown state while locked
        self.update_global_flags(cx, synced);
    }

    /// Finds the next task to run, this could be from a queue or stealing. If
    /// none are available, the thread sleeps and tries again.
    fn next_task(&mut self, cx: &Context, mut core: Box<Core>) -> NextTaskResult {
        self.assert_lifo_enabled_is_correct(cx);

        if self.is_traced {
            core = cx.handle.trace_core(core);
        }

        // Increment the tick
        self.tick = self.tick.wrapping_add(1);

        // Runs maintenance every so often. When maintenance is run, the
        // driver is checked, which may result in a task being found.
        core = try_task!(self.maybe_maintenance(&cx, core));

        // Check the LIFO slot, local run queue, and the injection queue for
        // a notified task.
        core = try_task!(self.next_notified_task(cx, core));

        // We consumed all work in the queues and will start searching for work.
        core.stats.end_processing_scheduled_tasks(&mut self.stats);

        super::counters::inc_num_no_local_work();

        if !cx.defer.borrow().is_empty() {
            // We are deferring tasks, so poll the resource driver and schedule
            // the deferred tasks.
            try_task_new_batch!(self, self.park_yield(cx, core));

            panic!("what happened to the deferred tasks? 🤔");
        }

        while !self.is_shutdown {
            // Search for more work, this involves trying to poll the resource
            // driver, steal from other workers, and check the global queue
            // again.
            core = try_task_new_batch!(self, self.search_for_work(cx, core));

            debug_assert!(cx.defer.borrow().is_empty());
            core = try_task_new_batch!(self, self.park(cx, core));
        }

        // Shutting down, drop any deferred tasks
        self.shutdown_clear_defer(cx);

        Ok((None, core))
    }

    fn next_notified_task(&mut self, cx: &Context, mut core: Box<Core>) -> NextTaskResult {
        self.num_seq_local_queue_polls += 1;

        if self.num_seq_local_queue_polls % self.global_queue_interval == 0 {
            super::counters::inc_global_queue_interval();

            self.num_seq_local_queue_polls = 0;

            // Update the global queue interval, if needed
            self.tune_global_queue_interval(cx, &mut core);

            if let Some(task) = self.next_remote_task(cx) {
                return Ok((Some(task), core));
            }
        }

        if let Some(task) = core.next_local_task() {
            return Ok((Some(task), core));
        }

        self.next_remote_task_batch(cx, core)
    }

    fn next_remote_task(&self, cx: &Context) -> Option<Notified> {
        if cx.shared().inject.is_empty() {
            return None;
        }

        let mut synced = cx.shared().synced.lock();
        cx.shared().next_remote_task_synced(&mut synced)
    }

    fn next_remote_task_batch(&self, cx: &Context, mut core: Box<Core>) -> NextTaskResult {
        if cx.shared().inject.is_empty() {
            return Ok((None, core));
        }

        // Other threads can only **remove** tasks from the current worker's
        // `run_queue`. So, we can be confident that by the time we call
        // `run_queue.push_back` below, there will be *at least* `cap`
        // available slots in the queue.
        let cap = usize::min(
            core.run_queue.remaining_slots(),
            usize::max(core.run_queue.max_capacity() / 2, 1),
        );

        let mut synced = cx.shared().synced.lock();
        let maybe_task = self.next_remote_task_batch_synced(cx, &mut synced, &mut core, cap);
        Ok((maybe_task, core))
    }

    fn next_remote_task_batch_synced(
        &self,
        cx: &Context,
        synced: &mut Synced,
        core: &mut Core,
        max: usize,
    ) -> Option<Notified> {
        super::counters::inc_num_remote_batch();

        // The worker is currently idle, pull a batch of work from the
        // injection queue. We don't want to pull *all* the work so other
        // workers can also get some.
        let n = if core.is_searching {
            cx.shared().inject.len() / cx.shared().idle.num_searching() + 1
        } else {
            cx.shared().inject.len() / cx.shared().remotes.len() + 1
        };

        let n = usize::min(n, max) + 1;

        // safety: passing in the correct `inject::Synced`.
        let mut tasks = unsafe { cx.shared().inject.pop_n(&mut synced.inject, n) };

        // Pop the first task to return immediately
        let ret = tasks.next();

        // Push the rest of the on the run queue
        core.run_queue.push_back(tasks);

        ret
    }

    /// Function responsible for stealing tasks from another worker
    ///
    /// Note: Only if less than half the workers are searching for tasks to steal
    /// a new worker will actually try to steal. The idea is to make sure not all
    /// workers will be trying to steal at the same time.
    fn search_for_work(&mut self, cx: &Context, mut core: Box<Core>) -> NextTaskResult {
        #[cfg(not(loom))]
        const ROUNDS: usize = 4;

        #[cfg(loom)]
        const ROUNDS: usize = 1;

        debug_assert!(core.lifo_slot.is_none());
        #[cfg(not(loom))]
        debug_assert!(core.run_queue.is_empty());

        if !core.run_queue.can_steal() {
            return Ok((None, core));
        }

        if !self.transition_to_searching(cx, &mut core) {
            return Ok((None, core));
        }

        // core = try_task!(self, self.poll_driver(cx, core));

        // Get a snapshot of which workers are idle
        cx.shared().idle.snapshot(&mut self.idle_snapshot);

        let num = cx.shared().remotes.len();

        for i in 0..ROUNDS {
            // Start from a random worker
            let start = core.rand.fastrand_n(num as u32) as usize;

            if let Some(task) = self.steal_one_round(cx, &mut core, start) {
                return Ok((Some(task), core));
            }

            core = try_task!(self.next_remote_task_batch(cx, core));

            if i > 0 {
                super::counters::inc_num_spin_stall();
                std::thread::sleep(std::time::Duration::from_micros(i as u64));
            }
        }

        Ok((None, core))
    }

    fn steal_one_round(&self, cx: &Context, core: &mut Core, start: usize) -> Option<Notified> {
        let num = cx.shared().remotes.len();

        for i in 0..num {
            let i = (start + i) % num;

            // Don't steal from ourself! We know we don't have work.
            if i == core.index {
                continue;
            }

            // If the core is currently idle, then there is nothing to steal.
            if self.idle_snapshot.is_idle(i) {
                continue;
            }

            let target = &cx.shared().remotes[i];

            if let Some(task) = target
                .steal
                .steal_into(&mut core.run_queue, &mut core.stats)
            {
                return Some(task);
            }
        }

        None
    }

    fn run_task(&mut self, cx: &Context, mut core: Box<Core>, task: Notified) -> RunResult {
        let task = cx.shared().owned.assert_owner(task);

        // Make sure the worker is not in the **searching** state. This enables
        // another idle worker to try to steal work.
        if self.transition_from_searching(cx, &mut core) {
            super::counters::inc_num_relay_search();
            cx.shared().notify_parked_local();
        }

        self.assert_lifo_enabled_is_correct(cx);

        // Measure the poll start time. Note that we may end up polling other
        // tasks under this measurement. In this case, the tasks came from the
        // LIFO slot and are considered part of the current task for scheduling
        // purposes. These tasks inherent the "parent"'s limits.
        core.stats.start_poll(&mut self.stats);

        // Make the core available to the runtime context
        *cx.core.borrow_mut() = Some(core);

        // Run the task
        coop::budget(|| {
            super::counters::inc_num_polls();
            task.run();
            let mut lifo_polls = 0;

            // As long as there is budget remaining and a task exists in the
            // `lifo_slot`, then keep running.
            loop {
                // Check if we still have the core. If not, the core was stolen
                // by another worker.
                let mut core = match cx.core.borrow_mut().take() {
                    Some(core) => core,
                    None => {
                        // In this case, we cannot call `reset_lifo_enabled()`
                        // because the core was stolen. The stealer will handle
                        // that at the top of `Context::run`
                        return Err(());
                    }
                };

                // Check for a task in the LIFO slot
                let task = match core.next_lifo_task() {
                    Some(task) => task,
                    None => {
                        self.reset_lifo_enabled(cx);
                        core.stats.end_poll();
                        return Ok(core);
                    }
                };

                if !coop::has_budget_remaining() {
                    core.stats.end_poll();

                    // Not enough budget left to run the LIFO task, push it to
                    // the back of the queue and return.
                    core.run_queue
                        .push_back_or_overflow(task, cx.shared(), &mut core.stats);
                    // If we hit this point, the LIFO slot should be enabled.
                    // There is no need to reset it.
                    debug_assert!(cx.lifo_enabled.get());
                    return Ok(core);
                }

                // Track that we are about to run a task from the LIFO slot.
                lifo_polls += 1;
                super::counters::inc_lifo_schedules();

                // Disable the LIFO slot if we reach our limit
                //
                // In ping-ping style workloads where task A notifies task B,
                // which notifies task A again, continuously prioritizing the
                // LIFO slot can cause starvation as these two tasks will
                // repeatedly schedule the other. To mitigate this, we limit the
                // number of times the LIFO slot is prioritized.
                if lifo_polls >= MAX_LIFO_POLLS_PER_TICK {
                    cx.lifo_enabled.set(false);
                    super::counters::inc_lifo_capped();
                }

                // Run the LIFO task, then loop
                *cx.core.borrow_mut() = Some(core);
                let task = cx.shared().owned.assert_owner(task);
                super::counters::inc_num_lifo_polls();
                task.run();
            }
        })
    }

    fn schedule_deferred_with_core<'a>(
        &mut self,
        cx: &'a Context,
        mut core: Box<Core>,
        synced: impl FnOnce() -> MutexGuard<'a, Synced>,
    ) -> NextTaskResult {
        let mut defer = cx.defer.borrow_mut();

        // Grab a task to run next
        let task = defer.pop();

        if task.is_none() {
            return Ok((None, core));
        }

        if !defer.is_empty() {
            let mut synced = synced();

            // Number of tasks we want to try to spread across idle workers
            let num_fanout = cmp::min(defer.len(), cx.shared().idle.num_idle(&synced.idle));

            // Cap the number of threads woken up at one time. This is to limit
            // the number of no-op wakes and reduce mutext contention.
            //
            // This number was picked after some basic benchmarks, but it can
            // probably be tuned using the mean poll time value (slower task
            // polls can leverage more woken workers).
            let num_fanout = cmp::min(2, num_fanout);

            if num_fanout > 0 {
                cx.shared()
                    .push_remote_task_batch_synced(&mut synced, defer.drain(..num_fanout));

                cx.shared()
                    .idle
                    .notify_mult(&mut synced, &mut self.workers_to_notify, num_fanout);
            }

            // Do not run the task while holding the lock...
            drop(synced);
        }

        // Notify any workers
        for worker in self.workers_to_notify.drain(..) {
            cx.shared().condvars[worker].notify_one()
        }

        if !defer.is_empty() {
            // Push the rest of the tasks on the local queue
            for task in defer.drain(..) {
                core.run_queue
                    .push_back_or_overflow(task, cx.shared(), &mut core.stats);
            }

            cx.shared().notify_parked_local();
        }

        Ok((task, core))
    }

    fn schedule_deferred_without_core<'a>(&mut self, cx: &Context, synced: &mut Synced) {
        let mut defer = cx.defer.borrow_mut();
        let num = defer.len();

        if num > 0 {
            // Push all tasks to the injection queue
            cx.shared()
                .push_remote_task_batch_synced(synced, defer.drain(..));

            debug_assert!(self.workers_to_notify.is_empty());

            // Notify workers
            cx.shared()
                .idle
                .notify_mult(synced, &mut self.workers_to_notify, num);

            // Notify any workers
            for worker in self.workers_to_notify.drain(..) {
                cx.shared().condvars[worker].notify_one()
            }
        }
    }

    fn maybe_maintenance(&mut self, cx: &Context, mut core: Box<Core>) -> NextTaskResult {
        if self.tick % cx.shared().config.event_interval == 0 {
            super::counters::inc_num_maintenance();

            core.stats.end_processing_scheduled_tasks(&mut self.stats);

            // Run regularly scheduled maintenance
            core = try_task_new_batch!(self, self.park_yield(cx, core));

            core.stats.start_processing_scheduled_tasks(&mut self.stats);
        }

        Ok((None, core))
    }

    fn flush_metrics(&self, cx: &Context, core: &mut Core) {
        core.stats.submit(&cx.shared().worker_metrics[core.index]);
    }

    fn update_global_flags(&mut self, cx: &Context, synced: &mut Synced) {
        if !self.is_shutdown {
            self.is_shutdown = cx.shared().inject.is_closed(&synced.inject);
        }

        if !self.is_traced {
            self.is_traced = cx.shared().trace_status.trace_requested();
        }
    }

    fn park_yield(&mut self, cx: &Context, core: Box<Core>) -> NextTaskResult {
        // Call `park` with a 0 timeout. This enables the I/O driver, timer, ...
        // to run without actually putting the thread to sleep.
        if let Some(mut driver) = cx.shared().driver.take() {
            driver.park_timeout(&cx.handle.driver, Duration::from_millis(0));

            cx.shared().driver.set(driver);
        }

        // If there are more I/O events, schedule them.
        let (maybe_task, mut core) =
            self.schedule_deferred_with_core(cx, core, || cx.shared().synced.lock())?;

        self.flush_metrics(cx, &mut core);
        self.update_global_flags(cx, &mut cx.shared().synced.lock());

        Ok((maybe_task, core))
    }

    /*
    fn poll_driver(&mut self, cx: &Context, core: Box<Core>) -> NextTaskResult {
        // Call `park` with a 0 timeout. This enables the I/O driver, timer, ...
        // to run without actually putting the thread to sleep.
        if let Some(mut driver) = cx.shared().driver.take() {
            driver.park_timeout(&cx.handle.driver, Duration::from_millis(0));

            cx.shared().driver.set(driver);

            // If there are more I/O events, schedule them.
            self.schedule_deferred_with_core(cx, core, || cx.shared().synced.lock())
        } else {
            Ok((None, core))
        }
    }
    */

    fn park(&mut self, cx: &Context, mut core: Box<Core>) -> NextTaskResult {
        if let Some(f) = &cx.shared().config.before_park {
            f();
        }

        if self.can_transition_to_parked(&mut core) {
            debug_assert!(!self.is_shutdown);
            debug_assert!(!self.is_traced);

            core = try_task!(self.do_park(cx, core));
        }

        if let Some(f) = &cx.shared().config.after_unpark {
            f();
        }

        Ok((None, core))
    }

    fn do_park(&mut self, cx: &Context, mut core: Box<Core>) -> NextTaskResult {
        let was_searching = core.is_searching;

        // Acquire the lock
        let mut synced = cx.shared().synced.lock();

        // The local queue should be empty at this point
        #[cfg(not(loom))]
        debug_assert!(core.run_queue.is_empty());

        // Try one last time to get tasks
        let n = cmp::max(core.run_queue.remaining_slots() / 2, 1);
        if let Some(task) = self.next_remote_task_batch_synced(cx, &mut synced, &mut core, n) {
            return Ok((Some(task), core));
        }

        if !was_searching {
            if cx
                .shared()
                .idle
                .transition_worker_to_searching_if_needed(&mut synced.idle, &mut core)
            {
                // Skip parking, go back to searching
                return Ok((None, core));
            }
        }

        super::counters::inc_num_parks();
        core.stats.about_to_park();
        // Flush metrics to the runtime metrics aggregator
        self.flush_metrics(cx, &mut core);

        // If the runtime is shutdown, skip parking
        self.update_global_flags(cx, &mut synced);

        if self.is_shutdown {
            return Ok((None, core));
        }

        // Release the core
        core.is_searching = false;
        cx.shared().idle.release_core(&mut synced, core);

        drop(synced);

        if was_searching {
            if cx.shared().idle.transition_worker_from_searching() {
                // cx.shared().idle.snapshot(&mut self.idle_snapshot);
                // We were the last searching worker, we need to do one last check
                for i in 0..cx.shared().remotes.len() {
                    if !cx.shared().remotes[i].steal.is_empty() {
                        let mut synced = cx.shared().synced.lock();

                        // Try to get a core
                        if let Some(mut core) = self.try_acquire_available_core(cx, &mut synced) {
                            cx.shared().idle.transition_worker_to_searching(&mut core);
                            return Ok((None, core));
                        } else {
                            // Fall back to the park routine
                            break;
                        }
                    }
                }
            }
        }

        if let Some(mut driver) = cx.shared().take_driver() {
            // Wait for driver events
            driver.park(&cx.handle.driver);

            synced = cx.shared().synced.lock();

            if cx.shared().inject.is_closed(&mut synced.inject) {
                synced.shutdown_driver = Some(driver);
                self.shutdown_clear_defer(cx);
                cx.shared().shutdown_finalize(&cx.handle, &mut synced);
                return Err(());
            }

            // Put the driver back
            cx.shared().driver.set(driver);

            // Try to acquire an available core to schedule I/O events
            if let Some(core) = self.try_acquire_available_core(cx, &mut synced) {
                // This may result in a task being run
                self.schedule_deferred_with_core(cx, core, move || synced)
            } else {
                // Schedule any deferred tasks
                self.schedule_deferred_without_core(cx, &mut synced);

                // Wait for a core.
                self.wait_for_core(cx, synced)
            }
        } else {
            synced = cx.shared().synced.lock();

            // Wait for a core to be assigned to us
            self.wait_for_core(cx, synced)
        }
    }

    fn transition_to_searching(&self, cx: &Context, core: &mut Core) -> bool {
        if !core.is_searching {
            cx.shared().idle.try_transition_worker_to_searching(core);
        }

        core.is_searching
    }

    /// Returns `true` if another worker must be notified
    fn transition_from_searching(&self, cx: &Context, core: &mut Core) -> bool {
        if !core.is_searching {
            return false;
        }

        core.is_searching = false;
        cx.shared().idle.transition_worker_from_searching()
    }

    fn can_transition_to_parked(&self, core: &mut Core) -> bool {
        !self.has_tasks(core) && !self.is_shutdown && !self.is_traced
    }

    fn has_tasks(&self, core: &Core) -> bool {
        core.lifo_slot.is_some() || !core.run_queue.is_empty()
    }

    fn reset_lifo_enabled(&self, cx: &Context) {
        cx.lifo_enabled
            .set(!cx.handle.shared.config.disable_lifo_slot);
    }

    fn assert_lifo_enabled_is_correct(&self, cx: &Context) {
        debug_assert_eq!(
            cx.lifo_enabled.get(),
            !cx.handle.shared.config.disable_lifo_slot
        );
    }

    fn tune_global_queue_interval(&mut self, cx: &Context, core: &mut Core) {
        let next = core.stats.tuned_global_queue_interval(&cx.shared().config);

        // Smooth out jitter
        if u32::abs_diff(self.global_queue_interval, next) > 2 {
            self.global_queue_interval = next;
        }
    }

    fn shutdown_clear_defer(&self, cx: &Context) {
        let mut defer = cx.defer.borrow_mut();

        for task in defer.drain(..) {
            drop(task);
        }
    }
}

impl Context {
    pub(crate) fn defer(&self, waker: &Waker) {
        // TODO: refactor defer across all runtimes
        waker.wake_by_ref();
    }

    fn shared(&self) -> &Shared {
        &self.handle.shared
    }

    #[cfg_attr(not(feature = "time"), allow(dead_code))]
    pub(crate) fn get_worker_index(&self) -> usize {
        self.index
    }
}

impl Core {
    fn next_local_task(&mut self) -> Option<Notified> {
        self.next_lifo_task().or_else(|| self.run_queue.pop())
    }

    fn next_lifo_task(&mut self) -> Option<Notified> {
        self.lifo_slot.take()
    }
}

impl Shared {
    fn next_remote_task_synced(&self, synced: &mut Synced) -> Option<Notified> {
        // safety: we only have access to a valid `Synced` in this file.
        unsafe { self.inject.pop(&mut synced.inject) }
    }

    pub(super) fn schedule_task(&self, task: Notified, is_yield: bool) {
        use std::ptr;

        with_current(|maybe_cx| {
            if let Some(cx) = maybe_cx {
                // Make sure the task is part of the **current** scheduler.
                if ptr::eq(self, &cx.handle.shared) {
                    // And the current thread still holds a core
                    if let Some(core) = cx.core.borrow_mut().as_mut() {
                        if is_yield {
                            cx.defer.borrow_mut().push(task);
                        } else {
                            self.schedule_local(cx, core, task);
                        }
                    } else {
                        // This can happen if either the core was stolen
                        // (`block_in_place`) or the notification happens from
                        // the driver.
                        cx.defer.borrow_mut().push(task);
                    }
                    return;
                }
            }

            // Otherwise, use the inject queue.
            self.schedule_remote(task);
        })
    }

    fn schedule_local(&self, cx: &Context, core: &mut Core, task: Notified) {
        core.stats.inc_local_schedule_count();

        if cx.lifo_enabled.get() {
            // Push to the LIFO slot
            let prev = std::mem::replace(&mut core.lifo_slot, Some(task));
            // let prev = cx.shared().remotes[core.index].lifo_slot.swap_local(task);

            if let Some(prev) = prev {
                core.run_queue
                    .push_back_or_overflow(prev, self, &mut core.stats);
            } else {
                return;
            }
        } else {
            core.run_queue
                .push_back_or_overflow(task, self, &mut core.stats);
        }

        self.notify_parked_local();
    }

    fn notify_parked_local(&self) {
        super::counters::inc_num_inc_notify_local();
        self.idle.notify_local(self);
    }

    fn schedule_remote(&self, task: Notified) {
        super::counters::inc_num_notify_remote();
        self.scheduler_metrics.inc_remote_schedule_count();

        let mut synced = self.synced.lock();
        // Push the task in the
        self.push_remote_task(&mut synced, task);

        // Notify a worker. The mutex is passed in and will be released as part
        // of the method call.
        self.idle.notify_remote(synced, self);
    }

    pub(super) fn close(&self, handle: &Handle) {
        {
            let mut synced = self.synced.lock();

            if let Some(driver) = self.driver.take() {
                synced.shutdown_driver = Some(driver);
            }

            if !self.inject.close(&mut synced.inject) {
                return;
            }

            // Set the shutdown flag on all available cores
            self.idle.shutdown(&mut synced, self);
        }

        // Any unassigned cores need to be shutdown, but we have to first drop
        // the lock
        self.idle.shutdown_unassigned_cores(handle, self);
    }

    fn push_remote_task(&self, synced: &mut Synced, task: Notified) {
        // safety: passing in correct `idle::Synced`
        unsafe {
            self.inject.push(&mut synced.inject, task);
        }
    }

    fn push_remote_task_batch<I>(&self, iter: I)
    where
        I: Iterator<Item = task::Notified<Arc<Handle>>>,
    {
        unsafe {
            self.inject.push_batch(self, iter);
        }
    }

    fn push_remote_task_batch_synced<I>(&self, synced: &mut Synced, iter: I)
    where
        I: Iterator<Item = task::Notified<Arc<Handle>>>,
    {
        unsafe {
            self.inject.push_batch(&mut synced.inject, iter);
        }
    }

    fn take_driver(&self) -> Option<Box<Driver>> {
        if !self.driver_enabled() {
            return None;
        }

        self.driver.take()
    }

    fn driver_enabled(&self) -> bool {
        self.condvars.len() > self.remotes.len()
    }

    pub(super) fn shutdown_core(&self, handle: &Handle, mut core: Box<Core>) {
        // Start from a random inner list
        let start = core.rand.fastrand_n(self.owned.get_shard_size() as u32);
        self.owned.close_and_shutdown_all(start as usize);

        core.stats.submit(&self.worker_metrics[core.index]);

        let mut synced = self.synced.lock();
        synced.shutdown_cores.push(core);

        self.shutdown_finalize(handle, &mut synced);
    }

    pub(super) fn shutdown_finalize(&self, handle: &Handle, synced: &mut Synced) {
        // Wait for all cores
        if synced.shutdown_cores.len() != self.remotes.len() {
            return;
        }

        let driver = synced.shutdown_driver.take();

        if self.driver_enabled() && driver.is_none() {
            return;
        }

        debug_assert!(self.owned.is_empty());

        for mut core in synced.shutdown_cores.drain(..) {
            // Drain tasks from the local queue
            while core.next_local_task().is_some() {}
        }

        // Shutdown the driver
        if let Some(mut driver) = driver {
            driver.shutdown(&handle.driver);
        }

        // Drain the injection queue
        //
        // We already shut down every task, so we can simply drop the tasks. We
        // cannot call `next_remote_task()` because we already hold the lock.
        //
        // safety: passing in correct `idle::Synced`
        while let Some(task) = self.next_remote_task_synced(synced) {
            drop(task);
        }
    }
}

impl Overflow<Arc<Handle>> for Shared {
    fn push(&self, task: task::Notified<Arc<Handle>>) {
        self.push_remote_task(&mut self.synced.lock(), task);
    }

    fn push_batch<I>(&self, iter: I)
    where
        I: Iterator<Item = task::Notified<Arc<Handle>>>,
    {
        self.push_remote_task_batch(iter)
    }
}

impl<'a> Lock<inject::Synced> for &'a Shared {
    type Handle = SyncedGuard<'a>;

    fn lock(self) -> Self::Handle {
        SyncedGuard {
            lock: self.synced.lock(),
        }
    }
}

impl<'a> Lock<Synced> for &'a Shared {
    type Handle = SyncedGuard<'a>;

    fn lock(self) -> Self::Handle {
        SyncedGuard {
            lock: self.synced.lock(),
        }
    }
}

impl task::Schedule for Arc<Handle> {
    fn release(&self, task: &Task) -> Option<Task> {
        self.shared.owned.remove(task)
    }

    fn schedule(&self, task: Notified) {
        self.shared.schedule_task(task, false);
    }

    fn hooks(&self) -> TaskHarnessScheduleHooks {
        TaskHarnessScheduleHooks {
            task_terminate_callback: self.task_hooks.task_terminate_callback.clone(),
        }
    }

    fn yield_now(&self, task: Notified) {
        self.shared.schedule_task(task, true);
    }
}

impl AsMut<Synced> for Synced {
    fn as_mut(&mut self) -> &mut Synced {
        self
    }
}

pub(crate) struct SyncedGuard<'a> {
    lock: crate::loom::sync::MutexGuard<'a, Synced>,
}

impl<'a> AsMut<inject::Synced> for SyncedGuard<'a> {
    fn as_mut(&mut self) -> &mut inject::Synced {
        &mut self.lock.inject
    }
}

impl<'a> AsMut<Synced> for SyncedGuard<'a> {
    fn as_mut(&mut self) -> &mut Synced {
        &mut self.lock
    }
}

#[track_caller]
fn with_current<R>(f: impl FnOnce(Option<&Context>) -> R) -> R {
    use scheduler::Context::MultiThreadAlt;

    context::with_scheduler(|ctx| match ctx {
        Some(MultiThreadAlt(ctx)) => f(Some(ctx)),
        _ => f(None),
    })
}
