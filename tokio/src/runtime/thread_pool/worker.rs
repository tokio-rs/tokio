//! A scheduler is initialized with a fixed number of workers. Each worker is
//! driven by a thread. Each worker has a "core" which contains data such as the
//! run queue and other state. When `block_in_place` is called, the worker's
//! "core" is handed off to a new thread allowing the scheduler to continue to
//! make progress while the originating thread blocks.

use crate::loom::rand::seed;
use crate::loom::sync::{Arc, Mutex};
use crate::park::{Park, Unpark};
use crate::runtime;
use crate::runtime::park::{Parker, Unparker};
use crate::runtime::thread_pool::{AtomicCell, Idle};
use crate::runtime::{queue, task};
use crate::util::linked_list::LinkedList;
use crate::util::FastRand;

use std::cell::RefCell;
use std::time::Duration;

/// A scheduler worker
pub(super) struct Worker {
    /// Reference to shared state
    shared: Arc<Shared>,

    /// Index holding this worker's remote state
    index: usize,

    /// Used to hand-off a worker's core to another thread.
    core: AtomicCell<Core>,
}

/// Core data
struct Core {
    /// Used to schedule bookkeeping tasks every so often.
    tick: u8,

    /// When a task is scheduled from a worker, it is stored in this slot. The
    /// worker will check this slot for a task **before** checking the run
    /// queue. This effectively results in the **last** scheduled task to be run
    /// next (LIFO). This is an optimization for message passing patterns and
    /// helps to reduce latency.
    lifo_slot: Option<Notified>,

    /// The worker-local run queue.
    run_queue: queue::Local<Arc<Worker>>,

    /// True if the worker is currently searching for more work. Searching
    /// involves attempting to steal from other workers.
    is_searching: bool,

    /// True if the scheduler is being shutdown
    is_shutdown: bool,

    /// Tasks owned by the core
    tasks: LinkedList<Task>,

    /// Parker
    ///
    /// Stored in an `Option` as the parker is added / removed to make the
    /// borrow checker happy.
    park: Option<Parker>,

    /// Fast random number generator.
    rand: FastRand,
}

/// State shared across all workers
pub(super) struct Shared {
    /// Per-worker remote state. All other workers have access to this and is
    /// how they communicate between each other.
    remotes: Box<[Remote]>,

    /// Submit work to the scheduler while **not** currently on a worker thread.
    inject: queue::Inject<Arc<Worker>>,

    /// Coordinates idle workers
    idle: Idle,

    /// Workers have have observed the shutdown signal
    ///
    /// The core is **not** placed back in the worker to avoid it from being
    /// stolen by a thread that was spawned as part of `block_in_place`.
    shutdown_workers: Mutex<Vec<(Box<Core>, Arc<Worker>)>>,
}

/// Used to communicate with a worker from other threads.
struct Remote {
    /// Steal tasks from this worker.
    steal: queue::Steal<Arc<Worker>>,

    /// Transfers tasks to be released. Any worker pushes tasks, only the owning
    /// worker pops.
    pending_drop: task::TransferStack<Arc<Worker>>,

    /// Unparks the associated worker thread
    unpark: Unparker,
}

/// Thread-local context
struct Context {
    /// Worker
    worker: Arc<Worker>,

    /// Core data
    core: RefCell<Option<Box<Core>>>,
}

/// Starts the workers
pub(crate) struct Launch(Vec<Arc<Worker>>);

/// Running a task may consume the core. If the core is still available when
/// running the task completes, it is returned. Otherwise, the worker will need
/// to stop processing.
type RunResult = Result<Box<Core>, ()>;

/// A task handle
type Task = task::Task<Arc<Worker>>;

/// A notified task handle
type Notified = task::Notified<Arc<Worker>>;

// Tracks thread-local state
scoped_thread_local!(static CURRENT: Context);

pub(super) fn create(size: usize, park: Parker) -> (Arc<Shared>, Launch) {
    let mut cores = vec![];
    let mut remotes = vec![];

    // Create the local queues
    for _ in 0..size {
        let (steal, run_queue) = queue::local();

        let park = park.clone();
        let unpark = park.unpark();

        cores.push(Box::new(Core {
            tick: 0,
            lifo_slot: None,
            run_queue,
            is_searching: false,
            is_shutdown: false,
            tasks: LinkedList::new(),
            park: Some(park),
            rand: FastRand::new(seed()),
        }));

        remotes.push(Remote {
            steal,
            pending_drop: task::TransferStack::new(),
            unpark,
        });
    }

    let shared = Arc::new(Shared {
        remotes: remotes.into_boxed_slice(),
        inject: queue::Inject::new(),
        idle: Idle::new(size),
        shutdown_workers: Mutex::new(vec![]),
    });

    let mut launch = Launch(vec![]);

    for (index, core) in cores.drain(..).enumerate() {
        launch.0.push(Arc::new(Worker {
            shared: shared.clone(),
            index,
            core: AtomicCell::new(Some(core)),
        }));
    }

    (shared, launch)
}

cfg_blocking! {
    pub(crate) fn block_in_place<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // Try to steal the worker core back
        struct Reset;

        impl Drop for Reset {
            fn drop(&mut self) {
                CURRENT.with(|maybe_cx| {
                    if let Some(cx) = maybe_cx {
                        let core = cx.worker.core.take();
                        *cx.core.borrow_mut() = core;
                    }
                });
            }
        }

        CURRENT.with(|maybe_cx| {
            let cx = maybe_cx.expect("can call blocking only when running in a spawned task");

            // Get the worker core. If none is set, then blocking is fine!
            let core = match cx.core.borrow_mut().take() {
                Some(core) => {
                    // We are effectively leaving the executor, so we need to
                    // forcibly end budgeting.
                    crate::coop::stop();
                    core
                },
                None => return,
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
        });

        let _reset = Reset;

        f()
    }
}

/// After how many ticks is the global queue polled. This helps to ensure
/// fairness.
///
/// The number is fairly arbitrary. I believe this value was copied from golang.
const GLOBAL_POLL_INTERVAL: u8 = 61;

impl Launch {
    pub(crate) fn launch(mut self) {
        for worker in self.0.drain(..) {
            runtime::spawn_blocking(move || run(worker));
        }
    }
}

fn run(worker: Arc<Worker>) {
    // Acquire a core. If this fails, then another thread is running this
    // worker and there is nothing further to do.
    let core = match worker.core.take() {
        Some(core) => core,
        None => return,
    };

    // Set the worker context.
    let cx = Context {
        worker,
        core: RefCell::new(None),
    };

    let _enter = crate::runtime::enter();

    CURRENT.set(&cx, || {
        // This should always be an error. It only returns a `Result` to support
        // using `?` to short circuit.
        assert!(cx.run(core).is_err());
    });
}

impl Context {
    fn run(&self, mut core: Box<Core>) -> RunResult {
        while !core.is_shutdown {
            // Increment the tick
            core.tick();

            // Run maintenance, if needed
            core = self.maintenance(core);

            // First, check work available to the current worker.
            if let Some(task) = core.next_task(&self.worker) {
                core = self.run_task(task, core)?;
                continue;
            }

            // There is no more **local** work to process, try to steal work
            // from other workers.
            if let Some(task) = core.steal_work(&self.worker) {
                core = self.run_task(task, core)?;
            } else {
                // Wait for work
                core = self.park(core);
            }
        }

        // Signal shutdown
        self.worker.shared.shutdown(core, self.worker.clone());
        Err(())
    }

    fn run_task(&self, task: Notified, mut core: Box<Core>) -> RunResult {
        // Make sure thew orker is not in the **searching** state. This enables
        // another idle worker to try to steal work.
        core.transition_from_searching(&self.worker);

        // Make the core available to the runtime context
        *self.core.borrow_mut() = Some(core);

        // Run the task
        crate::coop::budget(|| {
            task.run();

            // As long as there is budget remaining and a task exists in the
            // `lifo_slot`, then keep running.
            loop {
                // Check if we still have the core. If not, the core was stolen
                // by another worker.
                let mut core = match self.core.borrow_mut().take() {
                    Some(core) => core,
                    None => return Err(()),
                };

                // Check for a task in the LIFO slot
                let task = match core.lifo_slot.take() {
                    Some(task) => task,
                    None => return Ok(core),
                };

                if crate::coop::has_budget_remaining() {
                    // Run the LIFO task, then loop
                    *self.core.borrow_mut() = Some(core);
                    task.run();
                } else {
                    // Not enough budget left to run the LIFO task, push it to
                    // the back of the queue and return.
                    core.run_queue.push_back(task, self.worker.inject());
                    return Ok(core);
                }
            }
        })
    }

    fn maintenance(&self, mut core: Box<Core>) -> Box<Core> {
        if core.tick % GLOBAL_POLL_INTERVAL == 0 {
            // Call `park` with a 0 timeout. This enables the I/O driver, timer, ...
            // to run without actually putting the thread to sleep.
            core = self.park_timeout(core, Some(Duration::from_millis(0)));

            // Run regularly scheduled maintenance
            core.maintenance(&self.worker);
        }

        core
    }

    fn park(&self, mut core: Box<Core>) -> Box<Core> {
        core.transition_to_parked(&self.worker);

        while !core.is_shutdown {
            core = self.park_timeout(core, None);

            // Run regularly scheduled maintenance
            core.maintenance(&self.worker);

            if core.transition_from_parked(&self.worker) {
                return core;
            }
        }

        core
    }

    fn park_timeout(&self, mut core: Box<Core>, duration: Option<Duration>) -> Box<Core> {
        // Take the parker out of core
        let mut park = core.park.take().expect("park missing");

        // Store `core` in context
        *self.core.borrow_mut() = Some(core);

        // Park thread
        if let Some(timeout) = duration {
            park.park_timeout(timeout).expect("park failed");
        } else {
            park.park().expect("park failed");
        }

        // Remove `core` from context
        core = self.core.borrow_mut().take().expect("core missing");

        // Place `park` back in `core`
        core.park = Some(park);

        // If there are tasks available to steal, notify a worker
        if core.run_queue.is_stealable() {
            self.worker.shared.notify_parked();
        }

        core
    }
}

impl Core {
    /// Increment the tick
    fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Return the next notified task available to this worker.
    fn next_task(&mut self, worker: &Worker) -> Option<Notified> {
        if self.tick % GLOBAL_POLL_INTERVAL == 0 {
            worker.inject().pop().or_else(|| self.next_local_task())
        } else {
            self.next_local_task().or_else(|| worker.inject().pop())
        }
    }

    fn next_local_task(&mut self) -> Option<Notified> {
        self.lifo_slot.take().or_else(|| self.run_queue.pop())
    }

    fn steal_work(&mut self, worker: &Worker) -> Option<Notified> {
        if !self.transition_to_searching(worker) {
            return None;
        }

        let num = worker.shared.remotes.len();
        // Start from a random worker
        let start = self.rand.fastrand_n(num as u32) as usize;

        for i in 0..num {
            let i = (start + i) % num;

            // Don't steal from ourself! We know we don't have work.
            if i == worker.index {
                continue;
            }

            let target = &worker.shared.remotes[i];
            if let Some(task) = target.steal.steal_into(&mut self.run_queue) {
                return Some(task);
            }
        }

        // Fallback on checking the global queue
        worker.shared.inject.pop()
    }

    fn transition_to_searching(&mut self, worker: &Worker) -> bool {
        if !self.is_searching {
            self.is_searching = worker.shared.idle.transition_worker_to_searching();
        }

        self.is_searching
    }

    fn transition_from_searching(&mut self, worker: &Worker) {
        if !self.is_searching {
            return;
        }

        self.is_searching = false;
        worker.shared.transition_worker_from_searching();
    }

    /// Prepare the worker state for parking
    fn transition_to_parked(&mut self, worker: &Worker) {
        // When the final worker transitions **out** of searching to parked, it
        // must check all the queues one last time in case work materialized
        // between the last work scan and transitioning out of searching.
        let is_last_searcher = worker
            .shared
            .idle
            .transition_worker_to_parked(worker.index, self.is_searching);

        // The worker is no longer searching. Setting this is the local cache
        // only.
        self.is_searching = false;

        if is_last_searcher {
            worker.shared.notify_if_work_pending();
        }
    }

    /// Returns `true` if the transition happened.
    fn transition_from_parked(&mut self, worker: &Worker) -> bool {
        // If a task is in the lifo slot, then we must unpark regardless of
        // being notified
        if self.lifo_slot.is_some() {
            worker.shared.idle.unpark_worker_by_id(worker.index);
            self.is_searching = true;
            return true;
        }

        if worker.shared.idle.is_parked(worker.index) {
            return false;
        }

        // When unparked, the worker is in the searching state.
        self.is_searching = true;
        true
    }

    /// Runs maintenance work such as free pending tasks and check the pool's
    /// state.
    fn maintenance(&mut self, worker: &Worker) {
        self.drain_pending_drop(worker);

        if !self.is_shutdown {
            // Check if the scheduler has been shutdown
            self.is_shutdown = worker.inject().is_closed();
        }
    }

    // Shutdown the core
    fn shutdown(&mut self, worker: &Worker) {
        // Take the core
        let mut park = self.park.take().expect("park missing");

        // Signal to all tasks to shut down.
        for header in self.tasks.iter() {
            header.shutdown();
        }

        loop {
            self.drain_pending_drop(worker);

            if self.tasks.is_empty() {
                break;
            }

            // Wait until signalled
            park.park().expect("park failed");
        }

        // Drain the queue
        while let Some(_) = self.next_local_task() {}
    }

    fn drain_pending_drop(&mut self, worker: &Worker) {
        use std::mem::ManuallyDrop;

        for task in worker.remote().pending_drop.drain() {
            let task = ManuallyDrop::new(task);

            // safety: tasks are only pushed into the `pending_drop` stacks that
            // are associated with the list they are inserted into. When a task
            // is pushed into `pending_drop`, the ref-inc is skipped, so we must
            // not ref-dec here.
            //
            // See `bind` and `release` implementations.
            unsafe {
                self.tasks.remove(task.header().into());
            }
        }
    }
}

impl Worker {
    /// Returns a reference to the scheduler's injection queue
    fn inject(&self) -> &queue::Inject<Arc<Worker>> {
        &self.shared.inject
    }

    /// Return a reference to this worker's remote data
    fn remote(&self) -> &Remote {
        &self.shared.remotes[self.index]
    }

    fn eq(&self, other: &Worker) -> bool {
        self.shared.ptr_eq(&other.shared) && self.index == other.index
    }
}

impl task::Schedule for Arc<Worker> {
    fn bind(task: Task) -> Arc<Worker> {
        CURRENT.with(|maybe_cx| {
            let cx = maybe_cx.expect("scheduler context missing");

            // Track the task
            cx.core
                .borrow_mut()
                .as_mut()
                .expect("scheduler core missing")
                .tasks
                .push_front(task);

            // Return a clone of the worker
            cx.worker.clone()
        })
    }

    fn release(&self, task: &Task) -> Option<Task> {
        use std::ptr::NonNull;

        CURRENT.with(|maybe_cx| {
            let cx = maybe_cx.expect("scheduler context missing");

            if self.eq(&cx.worker) {
                let mut maybe_core = cx.core.borrow_mut();

                if let Some(core) = &mut *maybe_core {
                    // Directly remove the task
                    //
                    // safety: the task is inserted in the list in `bind`.
                    unsafe {
                        let ptr = NonNull::from(task.header());
                        return core.tasks.remove(ptr);
                    }
                }
            }

            // Track the task to be released by the worker that owns it
            //
            // Safety: We get a new handle without incrementing the ref-count.
            // A ref-count is held by the "owned" linked list and it is only
            // ever removed from that list as part of the release process: this
            // method or popping the task from `pending_drop`. Thus, we can rely
            // on the ref-count held by the linked-list to keep the memory
            // alive.
            //
            // When the task is removed from the stack, it is forgotten instead
            // of dropped.
            let task = unsafe { Task::from_raw(task.header().into()) };

            self.remote().pending_drop.push(task);

            if cx.core.borrow().is_some() {
                return None;
            }

            // The worker core has been handed off to another thread. In the
            // event that the scheduler is currently shutting down, the thread
            // that owns the task may be waiting on the release to complete
            // shutdown.
            if self.inject().is_closed() {
                self.remote().unpark.unpark();
            }

            None
        })
    }

    fn schedule(&self, task: Notified) {
        self.shared.schedule(task, false);
    }

    fn yield_now(&self, task: Notified) {
        self.shared.schedule(task, true);
    }
}

impl Shared {
    pub(super) fn schedule(&self, task: Notified, is_yield: bool) {
        CURRENT.with(|maybe_cx| {
            if let Some(cx) = maybe_cx {
                // Make sure the task is part of the **current** scheduler.
                if self.ptr_eq(&cx.worker.shared) {
                    // And the current thread still holds a core
                    if let Some(core) = cx.core.borrow_mut().as_mut() {
                        self.schedule_local(core, task, is_yield);
                        return;
                    }
                }
            }

            // Otherwise, use the inject queue
            self.inject.push(task);
            self.notify_parked();
        });
    }

    fn schedule_local(&self, core: &mut Core, task: Notified, is_yield: bool) {
        // Spawning from the worker thread. If scheduling a "yield" then the
        // task must always be pushed to the back of the queue, enabling other
        // tasks to be executed. If **not** a yield, then there is more
        // flexibility and the task may go to the front of the queue.
        let should_notify = if is_yield {
            core.run_queue.push_back(task, &self.inject);
            true
        } else {
            // Push to the LIFO slot
            let prev = core.lifo_slot.take();
            let ret = prev.is_some();

            if let Some(prev) = prev {
                core.run_queue.push_back(prev, &self.inject);
            }

            core.lifo_slot = Some(task);

            ret
        };

        // Only notify if not currently parked. If `park` is `None`, then the
        // scheduling is from a resource driver. As notifications often come in
        // batches, the notification is delayed until the park is complete.
        if should_notify && core.park.is_some() {
            self.notify_parked();
        }
    }

    pub(super) fn close(&self) {
        if self.inject.close() {
            self.notify_all();
        }
    }

    fn notify_parked(&self) {
        if let Some(index) = self.idle.worker_to_notify() {
            self.remotes[index].unpark.unpark();
        }
    }

    fn notify_all(&self) {
        for remote in &self.remotes[..] {
            remote.unpark.unpark();
        }
    }

    fn notify_if_work_pending(&self) {
        for remote in &self.remotes[..] {
            if !remote.steal.is_empty() {
                self.notify_parked();
                return;
            }
        }

        if !self.inject.is_empty() {
            self.notify_parked();
        }
    }

    fn transition_worker_from_searching(&self) {
        if self.idle.transition_worker_from_searching() {
            // We are the final searching worker. Because work was found, we
            // need to notify another worker.
            self.notify_parked();
        }
    }

    /// Signals that a worker has observed the shutdown signal and has replaced
    /// its core back into its handle.
    ///
    /// If all workers have reached this point, the final cleanup is performed.
    fn shutdown(&self, core: Box<Core>, worker: Arc<Worker>) {
        let mut workers = self.shutdown_workers.lock().unwrap();
        workers.push((core, worker));

        if workers.len() != self.remotes.len() {
            return;
        }

        for (mut core, worker) in workers.drain(..) {
            core.shutdown(&worker);
        }

        // Drain the injection queue
        while let Some(_) = self.inject.pop() {}
    }

    fn ptr_eq(&self, other: &Shared) -> bool {
        self as *const _ == other as *const _
    }
}
