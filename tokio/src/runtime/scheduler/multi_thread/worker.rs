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
//!     OwnedTasks instance and wakes up all worker threads.
//!
//!  2. Each worker thread observes the close signal next time it runs
//!     Core::maintenance by checking whether the inject queue is closed.
//!     The Core::is_shutdown flag is set to true.
//!
//!  3. The worker thread calls `pre_shutdown` in parallel. Here, the worker
//!     will keep removing tasks from OwnedTasks until it is empty. No new
//!     tasks can be pushed to the OwnedTasks during or after this step as it
//!     was closed in step 1.
//!
//!  5. The workers call Shared::shutdown to enter the single-threaded phase of
//!     shutdown. These calls will push their core to Shared::shutdown_cores,
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
//!  * The spawner observes the OwnedTasks being open, and the inject queue is
//!    closed.
//!  * The spawner observes the OwnedTasks being closed and doesn't check the
//!    inject queue.
//!
//! The first case can only happen if the OwnedTasks::bind call happens before
//! or during step 1 of shutdown. In this case, the runtime will clean up the
//! task in step 3 of shutdown.
//!
//! In the latter case, the task was not spawned and the task is immediately
//! cancelled by the spawner.
//!
//! The correctness of shutdown requires both the inject queue and OwnedTasks
//! collection to have a closed bit. With a close bit on only the inject queue,
//! spawning could run in to a situation where a task is successfully bound long
//! after the runtime has shut down. With a close bit on only the OwnedTasks,
//! the first spawning situation could result in the notification being pushed
//! to the inject queue after step 6 of shutdown, which would leave a task in
//! the inject queue indefinitely. This would be a ref-count cycle and a memory
//! leak.

use crate::loom::sync::{Arc, Condvar, Mutex, MutexGuard};
use crate::runtime;
use crate::runtime::context;
use crate::runtime::scheduler::multi_thread::{
    idle, queue, Counters, Handle, Idle, Overflow, Stats, TraceStatus,
};
use crate::runtime::scheduler::{self, inject, Lock};
use crate::runtime::task::OwnedTasks;
use crate::runtime::{
    blocking, coop, driver, task, Config, Driver, SchedulerMetrics, WorkerMetrics,
};
use crate::util::rand::{FastRand, RngSeedGenerator};

use std::cell::RefCell;
use std::cmp;
use std::task::Waker;

cfg_metrics! {
    mod metrics;
}

cfg_taskdump! {
    mod taskdump;
}

cfg_not_taskdump! {
    mod taskdump_mock;
}

/// A scheduler worker
pub(super) struct Worker {
    /// Reference to scheduler's handle
    handle: Arc<Handle>,

    /// This worker's index in `available_cores` and `condvars`.
    index: usize,

    /// Used to collect a list of workers to notify
    workers_to_notify: Vec<usize>,
}

/// Core data
pub(super) struct Core {
    /// Index holding this core's remote/shared state.
    index: usize,

    /// Used to schedule bookkeeping tasks every so often.
    tick: u32,

    /// When a task is scheduled from a worker, it is stored in this slot. The
    /// worker will check this slot for a task **before** checking the run
    /// queue. This effectively results in the **last** scheduled task to be run
    /// next (LIFO). This is an optimization for improving locality which
    /// benefits message passing patterns and helps to reduce latency.
    lifo_slot: Option<Notified>,

    /// When `true`, locally scheduled tasks go to the LIFO slot. When `false`,
    /// they go to the back of the `run_queue`.
    lifo_enabled: bool,

    /// The worker-local run queue.
    run_queue: queue::Local<Arc<Handle>>,

    /// True if the worker is currently searching for more work. Searching
    /// involves attempting to steal from other workers.
    pub(super) is_searching: bool,

    /// True if the scheduler is being shutdown
    is_shutdown: bool,

    /// True if the scheduler is being traced
    is_traced: bool,

    /// Per-worker runtime stats
    stats: Stats,

    /// How often to check the global queue
    global_queue_interval: u32,

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

    /// Condition variables used to unblock worker threads. Each worker thread
    /// has its own condvar it waits on.
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
    /// the `tokio_internal_mt_counters` cfg flag is set.
    _counters: Counters,
}

/// Data synchronized by the scheduler mutex
pub(crate) struct Synced {
    /// Cores not currently assigned to workers
    pub(super) available_cores: Vec<Box<Core>>,

    /// When worker is notified, it is assigned a core. The core is placed here
    /// until the worker wakes up to take it.
    pub(super) assigned_cores: Vec<Option<Box<Core>>>,

    /// Cores that have observed the shutdown signal
    ///
    /// The core is **not** placed back in the worker to avoid it from being
    /// stolen by a thread that was spawned as part of `block_in_place`.
    shutdown_cores: Vec<Box<Core>>,

    /// Synchronized state for `Idle`.
    pub(super) idle: idle::Synced,

    /// Synchronized state for `Inject`.
    pub(crate) inject: inject::Synced,

    /// Power's Tokio's I/O, timers, etc... the responsibility of polling the
    /// driver is shared across workers.
    driver: Option<Box<Driver>>,
}

/// Used to communicate with a worker from other threads.
struct Remote {
    /// Steals tasks from this worker.
    pub(super) steal: queue::Steal<Arc<Handle>>,
}

/// Thread-local context
pub(crate) struct Context {
    // Current scheduler's handle
    handle: Arc<Handle>,

    /// Core data
    core: RefCell<Option<Box<Core>>>,

    /// Tasks to wake after resource drivers are polled. This is mostly to
    /// handle yielded tasks.
    pub(crate) defer: RefCell<Vec<Notified>>,
}

/// Running a task may consume the core. If the core is still available when
/// running the task completes, it is returned. Otherwise, the worker will need
/// to stop processing.
type RunResult = Result<Box<Core>, ()>;

/// A task handle
type Task = task::Task<Arc<Handle>>;

/// A notified task handle
type Notified = task::Notified<Arc<Handle>>;

/// Value picked out of thin-air. Running the LIFO slot a handful of times
/// seemms sufficient to benefit from locality. More than 3 times probably is
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
    // Allocate num_cores + 1 workers so that one worker can handle the I/O
    // driver, if needed.
    let num_workers = num_cores + 1;
    let mut cores = Vec::with_capacity(num_cores);
    let mut remotes = Vec::with_capacity(num_cores);
    // Worker metrics are actually core based
    let mut worker_metrics = Vec::with_capacity(num_cores);

    // Create the local queues
    for i in 0..num_cores {
        let (steal, run_queue) = queue::local();

        let metrics = WorkerMetrics::from_config(&config);
        let stats = Stats::new(&metrics);

        cores.push(Box::new(Core {
            index: i,
            tick: 0,
            lifo_slot: None,
            lifo_enabled: !config.disable_lifo_slot,
            run_queue,
            is_searching: false,
            is_shutdown: false,
            is_traced: false,
            global_queue_interval: stats.tuned_global_queue_interval(&config),
            stats,
            rand: FastRand::from_seed(config.seed_generator.next_seed()),
        }));

        remotes.push(Remote { steal });
        worker_metrics.push(metrics);
    }

    // Allocate num-cores + 1 workers, so one worker can handle the I/O driver,
    // if needed.
    let (idle, idle_synced) = Idle::new(num_cores, num_workers);
    let (inject, inject_synced) = inject::Shared::new();

    let handle = Arc::new(Handle {
        shared: Shared {
            remotes: remotes.into_boxed_slice(),
            inject,
            idle,
            owned: OwnedTasks::new(),
            synced: Mutex::new(Synced {
                available_cores: cores,
                assigned_cores: Vec::with_capacity(num_cores),
                shutdown_cores: Vec::with_capacity(num_cores),
                idle: idle_synced,
                inject: inject_synced,
                driver: Some(Box::new(driver)),
            }),
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
        inner: scheduler::Handle::MultiThread(handle),
    };

    // Eagerly start worker threads
    for index in 0..num_workers {
        let handle = rt_handle.inner.expect_multi_thread();
        let worker = Worker {
            handle: handle.clone(),
            index,
            workers_to_notify: Vec::with_capacity(num_workers - 1),
        };

        handle
            .blocking_spawner
            .spawn_blocking(&rt_handle, move || run(worker));
    }

    rt_handle
}

#[track_caller]
pub(crate) fn block_in_place<F, R>(_f: F) -> R
where
    F: FnOnce() -> R,
{
    /*
    // Try to steal the worker core back
    struct Reset(coop::Budget);

    impl Drop for Reset {
        fn drop(&mut self) {
            with_current(|maybe_cx| {
                if let Some(cx) = maybe_cx {
                    let core = cx.worker.core.take();
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

        // The parker should be set here
        assert!(core.park.is_some());

        // In order to block, the core must be sent to another thread for
        // execution.
        //
        // First, move the core back into the worker's shared core slot.
        cx.worker.core.set(core);

        // Next, clone the worker handle and send it to a new thread for
        // processing.
        //
        // Once the blocking task is done executing, we will attempt to
        // steal the core back.
        let worker = cx.worker.clone();
        runtime::spawn_blocking(move || run(worker));
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
    */
    todo!()
}

fn run(mut worker: Worker) {
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

    let handle = scheduler::Handle::MultiThread(worker.handle.clone());

    crate::runtime::context::enter_runtime(&handle, true, |_| {
        // Set the worker context.
        let cx = scheduler::Context::MultiThread(Context {
            handle: worker.handle.clone(),
            core: RefCell::new(None),
            defer: RefCell::new(Vec::with_capacity(64)),
        });

        context::set_scheduler(&cx, || {
            let cx = cx.expect_multi_thread();

            // Run the worker
            worker.run(&cx);

            // Check if there are any deferred tasks to notify. This can happen when
            // the worker core is lost due to `block_in_place()` being called from
            // within the task.
            if !cx.defer.borrow().is_empty() {
                // cx.defer.wake();
                todo!()
            }
        });
    });
}

impl Worker {
    fn run(&mut self, cx: &Context) {
        let mut core = {
            let mut synced = cx.shared().synced.lock();

            // First try to acquire an available core
            if let Some(core) = self.try_acquire_available_core(cx, &mut synced) {
                core
            } else {
                // block the thread to wait for a core to be assinged to us
                match self.wait_for_core(cx, synced) {
                    Ok(core) => core,
                    Err(_) => return,
                }
            }
        };

        while !core.is_shutdown {
            self.assert_lifo_enabled_is_correct(&core);

            if core.is_traced {
                core = self.handle.trace_core(core);
            }

            // Increment the tick
            core.tick();

            // Run maintenance, if needed
            core = self.maybe_maintenance(cx, core);

            // First, check work available to the current worker.
            if let Some(task) = self.next_task(cx, &mut core) {
                core = match self.run_task(cx, core, task) {
                    Ok(core) => core,
                    Err(_) => return,
                };

                continue;
            }

            // We consumed all work in the queues and will start searching for work.
            core.stats.end_processing_scheduled_tasks();

            // There is no more **local** work to process, try to steal work
            // from other workers.
            if let Some(task) = self.steal_work(cx, &mut core) {
                // Found work, switch back to processing
                core.stats.start_processing_scheduled_tasks();

                core = match self.run_task(cx, core, task) {
                    Ok(core) => core,
                    Err(_) => return,
                };
            } else {
                // Wait for work
                core = if !cx.defer.borrow().is_empty() {
                    // Just run maintenance
                    self.park_yield(cx, core)
                } else {
                    match self.park(cx, core) {
                        Ok(core) => core,
                        Err(_) => return,
                    }
                };
            }
        }

        self.pre_shutdown(cx, &mut core);

        // Signal shutdown
        self.shutdown_core(cx, core);
    }

    // Try to acquire an available core, but do not block the thread
    fn try_acquire_available_core(&self, cx: &Context, synced: &mut Synced) -> Option<Box<Core>> {
        if let Some(mut core) = synced.available_cores.pop() {
            self.reset_acquired_core(cx, synced, &mut core);
            Some(core)
        } else {
            None
        }
    }

    // Block the current thread, waiting for an available core
    fn wait_for_core(&self, cx: &Context, mut synced: MutexGuard<'_, Synced>) -> RunResult {
        cx.shared()
            .idle
            .transition_worker_to_parked(&mut synced, self.index);

        // Wait until a core is available, then exit the loop.
        let mut core = loop {
            if let Some(core) = synced.assigned_cores[self.index].take() {
                break core;
            }

            synced = cx.shared().condvars[self.index].wait(synced).unwrap();
        };

        self.reset_acquired_core(cx, &mut synced, &mut core);

        if core.is_shutdown {
            // Currently shutting down, don't do any more work
            return Ok(core);
        }

        // The core was notified to search for work, don't try to take tasks from the injection queue
        if core.is_searching {
            return Ok(core);
        }

        // TODO: don't hardcode 128
        let n = core.run_queue.max_capacity() / 2;
        let maybe_task = self.next_remote_task_batch(cx, &mut synced, &mut core, n);

        drop(synced);

        // Start as "processing" tasks as polling tasks from the local queue
        // will be one of the first things we do.
        core.stats.start_processing_scheduled_tasks();

        if let Some(task) = maybe_task {
            self.run_task(cx, core, task)
        } else {
            Ok(core)
        }
    }

    /// Ensure core's state is set correctly for the worker to start using.
    fn reset_acquired_core(&self, cx: &Context, synced: &mut Synced, core: &mut Core) {
        // Reset `lifo_enabled` here in case the core was previously stolen from
        // a task that had the LIFO slot disabled.
        self.reset_lifo_enabled(core);

        // At this point, the local queue should be empty
        debug_assert!(core.run_queue.is_empty());

        // Update shutdown state while locked
        core.is_shutdown = cx.shared().inject.is_closed(&synced.inject);
    }

    fn next_task(&self, cx: &Context, core: &mut Core) -> Option<Notified> {
        if core.tick % core.global_queue_interval == 0 {
            // Update the global queue interval, if needed
            self.tune_global_queue_interval(cx, core);

            self.next_remote_task(cx)
                .or_else(|| self.next_local_task(core))
        } else {
            let maybe_task = self.next_local_task(core);

            if maybe_task.is_some() {
                return maybe_task;
            }

            if cx.shared().inject.is_empty() {
                return None;
            }

            // Other threads can only **remove** tasks from the current worker's
            // `run_queue`. So, we can be confident that by the time we call
            // `run_queue.push_back` below, there will be *at least* `cap`
            // available slots in the queue.
            let cap = usize::min(
                core.run_queue.remaining_slots(),
                core.run_queue.max_capacity() / 2,
            );

            let mut synced = cx.shared().synced.lock();
            self.next_remote_task_batch(cx, &mut synced, core, cap)
        }
    }

    fn next_remote_task(&self, cx: &Context) -> Option<Notified> {
        if cx.shared().inject.is_empty() {
            return None;
        }

        let mut synced = cx.shared().synced.lock();
        self.next_remote_task_synced(cx, &mut synced)
    }

    fn next_remote_task_synced(&self, cx: &Context, synced: &mut Synced) -> Option<Notified> {
        // safety: we only have access to a valid `Synced` in this file.
        unsafe { cx.shared().inject.pop(&mut synced.inject) }
    }

    fn next_remote_task_batch(
        &self,
        cx: &Context,
        synced: &mut Synced,
        core: &mut Core,
        max: usize,
    ) -> Option<Notified> {
        // The worker is currently idle, pull a batch of work from the
        // injection queue. We don't want to pull *all* the work so other
        // workers can also get some.
        let n = usize::min(
            cx.shared().inject.len() / cx.shared().remotes.len() + 1,
            max,
        );

        // safety: passing in the correct `inject::Synced`.
        let mut tasks = unsafe { cx.shared().inject.pop_n(&mut synced.inject, n) };

        // Pop the first task to return immedietly
        let ret = tasks.next();

        // Push the rest of the on the run queue
        core.run_queue.push_back(tasks);

        ret
    }

    fn next_local_task(&self, core: &mut Core) -> Option<Notified> {
        core.lifo_slot.take().or_else(|| core.run_queue.pop())
    }

    /// Function responsible for stealing tasks from another worker
    ///
    /// Note: Only if less than half the workers are searching for tasks to steal
    /// a new worker will actually try to steal. The idea is to make sure not all
    /// workers will be trying to steal at the same time.
    fn steal_work(&self, cx: &Context, core: &mut Core) -> Option<Notified> {
        if !self.transition_to_searching(cx, core) {
            return None;
        }

        let num = cx.shared().remotes.len();
        // Start from a random worker
        let start = core.rand.fastrand_n(num as u32) as usize;

        self.steal_one_round(cx, core, start)
    }

    fn steal_one_round(&self, cx: &Context, core: &mut Core, start: usize) -> Option<Notified> {
        let num = cx.shared().remotes.len();

        for i in 0..num {
            let i = (start + i) % num;

            // Don't steal from ourself! We know we don't have work.
            if i == core.index {
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

    fn run_task(&self, cx: &Context, mut core: Box<Core>, task: Notified) -> RunResult {
        let task = cx.shared().owned.assert_owner(task);

        // Make sure the worker is not in the **searching** state. This enables
        // another idle worker to try to steal work.
        if self.transition_from_searching(cx, &mut core) {
            cx.shared().notify_parked_local();
        }

        self.assert_lifo_enabled_is_correct(&core);

        // Measure the poll start time. Note that we may end up polling other
        // tasks under this measurement. In this case, the tasks came from the
        // LIFO slot and are considered part of the current task for scheduling
        // purposes. These tasks inherent the "parent"'s limits.
        core.stats.start_poll();

        // Make the core available to the runtime context
        *cx.core.borrow_mut() = Some(core);

        // Run the task
        coop::budget(|| {
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
                let task = match core.lifo_slot.take() {
                    Some(task) => task,
                    None => {
                        self.reset_lifo_enabled(&mut core);
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
                    debug_assert!(core.lifo_enabled);
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
                    core.lifo_enabled = false;
                    super::counters::inc_lifo_capped();
                }

                // Run the LIFO task, then loop
                *cx.core.borrow_mut() = Some(core);
                let task = cx.shared().owned.assert_owner(task);
                task.run();
            }
        })
    }

    fn schedule_deferred_with_core(
        &mut self,
        cx: &Context,
        core: Box<Core>,
        mut synced: MutexGuard<'_, Synced>,
    ) -> RunResult {
        let mut defer = cx.defer.borrow_mut();

        // Grab a task to run next
        let task = defer.pop();

        // Number of tasks we want to try to spread across idle workers
        let num_fanout = cmp::min(defer.len(), synced.available_cores.len());

        if num_fanout > 0 {
            cx.shared()
                .push_remote_task_batch_synced(&mut synced, defer.drain(..num_fanout));

            cx.shared()
                .idle
                .notify_mult(&mut synced, &mut self.workers_to_notify, num_fanout);
        }

        // Do not run the task while holding the lock...
        drop(synced);

        // Notify any workers
        for worker in self.workers_to_notify.drain(..) {
            cx.shared().condvars[worker].notify_one()
        }

        if let Some(task) = task {
            self.run_task(cx, core, task)
        } else {
            Ok(core)
        }
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

            debug_assert!(self.workers_to_notify.is_empty());
        }
    }

    fn maybe_maintenance(&self, cx: &Context, mut core: Box<Core>) -> Box<Core> {
        if core.tick % cx.shared().config.event_interval == 0 {
            super::counters::inc_num_maintenance();

            core.stats.end_processing_scheduled_tasks();

            // Run regularly scheduled maintenance
            self.maintenance(cx, &mut core);

            core.stats.start_processing_scheduled_tasks();
        }

        core
    }

    /// Runs maintenance work such as checking the pool's state.
    fn maintenance(&self, cx: &Context, core: &mut Core) {
        /*
        // Call `park` with a 0 timeout. This enables the I/O driver, timer, ...
        // to run without actually putting the thread to sleep.
        core = self.park_timeout(core, Some(Duration::from_millis(0)));
        */
        if true {
            todo!();
        }

        core.stats.submit(&cx.shared().worker_metrics[core.index]);

        if !core.is_shutdown {
            // Check if the scheduler has been shutdown
            let synced = cx.shared().synced.lock();
            core.is_shutdown = cx.shared().inject.is_closed(&synced.inject);
        }

        if !core.is_traced {
            // Check if the worker should be tracing.
            core.is_traced = cx.shared().trace_status.trace_requested();
        }
    }

    fn park_yield(&self, cx: &Context, mut core: Box<Core>) -> Box<Core> {
        /*
        // Call `park` with a 0 timeout. This enables the I/O driver, timer, ...
        // to run without actually putting the thread to sleep.
        core = self.park_timeout(core, Some(Duration::from_millis(0)));
        */
        if true {
            todo!();
        }

        self.maintenance(cx, &mut core);
        core
    }

    fn park(&mut self, cx: &Context, mut core: Box<Core>) -> RunResult {
        if let Some(f) = &cx.shared().config.before_park {
            f();
        }

        if self.can_transition_to_parked(&mut core) {
            debug_assert!(!core.is_shutdown);
            debug_assert!(!core.is_traced);

            core = self.do_park(cx, core)?;
        }

        if let Some(f) = &cx.shared().config.after_unpark {
            f();
        }

        Ok(core)
    }

    fn do_park(&mut self, cx: &Context, mut core: Box<Core>) -> RunResult {
        core.stats.about_to_park();

        let was_searching = core.is_searching;

        // Before we park, if we are searching, we need to transition away from searching
        if self.transition_from_searching(cx, &mut core) {
            // We were the last searching worker, we need to do one last check
            if let Some(task) = self.steal_one_round(cx, &mut core, 0) {
                cx.shared().notify_parked_local();

                return self.run_task(cx, core, task);
            }
        }

        // Acquire the lock
        let mut synced = cx.shared().synced.lock();

        // Try one last time to get tasks
        let n = core.run_queue.max_capacity() / 2;
        if let Some(task) = self.next_remote_task_batch(cx, &mut synced, &mut core, n) {
            drop(synced);

            return self.run_task(cx, core, task);
        }

        if !was_searching {
            if cx
                .shared()
                .idle
                .transition_worker_to_searching_if_needed(&mut synced.idle, &mut core)
            {
                // Skip parking, go back to searching
                return Ok(core);
            }
        }

        // Core being returned must not be in the searching state
        debug_assert!(!core.is_searching);

        // Release the core
        cx.shared().idle.release_core(&mut synced, core);

        if let Some(mut driver) = synced.driver.take() {
            // Drop the lock before parking on the driver
            drop(synced);

            // Wait for driver events
            driver.park(&self.handle.driver);

            synced = cx.shared().synced.lock();

            // Try to acquire an available core to schedule I/O events
            if let Some(core) = self.try_acquire_available_core(cx, &mut synced) {
                // This may result in a task being run
                self.schedule_deferred_with_core(cx, core, synced)
            } else {
                // Schedule any deferred tasks
                self.schedule_deferred_without_core(cx, &mut synced);

                // Wait for a core.
                self.wait_for_core(cx, synced)
            }
        } else {
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

        cx.shared().idle.transition_worker_from_searching(core)
    }

    fn can_transition_to_parked(&self, core: &mut Core) -> bool {
        core.lifo_slot.is_none() && core.run_queue.is_empty() && !core.is_traced
    }

    /// Signals all tasks to shut down, and waits for them to complete. Must run
    /// before we enter the single-threaded phase of shutdown processing.
    fn pre_shutdown(&self, cx: &Context, core: &mut Core) {
        // Signal to all tasks to shut down.
        cx.shared().owned.close_and_shutdown_all();

        core.stats.submit(&cx.shared().worker_metrics[core.index]);
    }

    /// Signals that a worker has observed the shutdown signal and has replaced
    /// its core back into its handle.
    ///
    /// If all workers have reached this point, the final cleanup is performed.
    fn shutdown_core(&self, cx: &Context, core: Box<Core>) {
        let mut synced = cx.shared().synced.lock();
        synced.shutdown_cores.push(core);

        if synced.shutdown_cores.len() != cx.shared().remotes.len() {
            return;
        }

        debug_assert!(cx.shared().owned.is_empty());

        for mut core in synced.shutdown_cores.drain(..) {
            // Drain tasks from the local queue
            while self.next_local_task(&mut core).is_some() {}
        }

        // Shutdown the driver
        let mut driver = synced.driver.take().expect("driver missing");
        driver.shutdown(&self.handle.driver);

        // Drain the injection queue
        //
        // We already shut down every task, so we can simply drop the tasks. We
        // cannot call `next_remote_task()` because we already hold the lock.
        //
        // safety: passing in correct `idle::Synced`
        while let Some(task) = self.next_remote_task_synced(cx, &mut synced) {
            drop(task);
        }
    }

    fn reset_lifo_enabled(&self, core: &mut Core) {
        core.lifo_enabled = !self.handle.shared.config.disable_lifo_slot;
    }

    fn assert_lifo_enabled_is_correct(&self, core: &Core) {
        debug_assert_eq!(
            core.lifo_enabled,
            !self.handle.shared.config.disable_lifo_slot
        );
    }

    fn tune_global_queue_interval(&self, cx: &Context, core: &mut Core) {
        let next = core.stats.tuned_global_queue_interval(&cx.shared().config);

        debug_assert!(next > 1);

        // Smooth out jitter
        if abs_diff(core.global_queue_interval, next) > 2 {
            core.global_queue_interval = next;
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
}

impl Shared {
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
                            self.schedule_local(core, task);
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

    fn schedule_local(&self, core: &mut Core, task: Notified) {
        // Push to the LIFO slot
        let prev = core.lifo_slot.take();

        if let Some(prev) = prev {
            core.run_queue
                .push_back_or_overflow(prev, self, &mut core.stats);
        }

        core.lifo_slot = Some(task);

        self.notify_parked_local();
    }

    fn notify_parked_local(&self) {
        super::counters::inc_num_inc_notify_local();
        self.idle.notify_local(self);
    }

    fn schedule_remote(&self, task: Notified) {
        self.scheduler_metrics.inc_remote_schedule_count();

        let mut synced = self.synced.lock();
        // Push the task in the
        self.push_remote_task(&mut synced, task);

        // Notify a worker. The mutex is passed in and will be released as part
        // of the method call.
        self.idle.notify_remote(synced, self);
    }

    pub(super) fn close(&self) {
        if self.inject.close(&mut self.synced.lock().inject) {
            // self.notify_all();
            todo!()
        }
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
    type Handle = InjectGuard<'a>;

    fn lock(self) -> Self::Handle {
        InjectGuard {
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

    fn yield_now(&self, task: Notified) {
        self.shared.schedule_task(task, true);
    }
}

impl Core {
    /// Increment the tick
    fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }
}

pub(crate) struct InjectGuard<'a> {
    lock: crate::loom::sync::MutexGuard<'a, Synced>,
}

impl<'a> AsMut<inject::Synced> for InjectGuard<'a> {
    fn as_mut(&mut self) -> &mut inject::Synced {
        &mut self.lock.inject
    }
}

#[track_caller]
fn with_current<R>(f: impl FnOnce(Option<&Context>) -> R) -> R {
    use scheduler::Context::MultiThread;

    context::with_scheduler(|ctx| match ctx {
        Some(MultiThread(ctx)) => f(Some(ctx)),
        _ => f(None),
    })
}

// `u32::abs_diff` is not available on Tokio's MSRV.
fn abs_diff(a: u32, b: u32) -> u32 {
    if a > b {
        a - b
    } else {
        b - a
    }
}
