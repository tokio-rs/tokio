use crate::park::{Park, Unpark};
use crate::task::{self, JoinHandle, Schedule, ScheduleSendOnly, Task};

use std::cell::{Cell, UnsafeCell};
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::ptr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{RawWaker, RawWakerVTable, Waker};
use std::time::Duration;

/// Executes tasks on the current thread
#[derive(Debug)]
pub(crate) struct BasicScheduler<P>
where
    P: Park,
{
    /// Scheduler component
    scheduler: Arc<SchedulerPriv>,

    /// Local state
    local: LocalState<P>,
}

#[derive(Debug, Clone)]
pub(crate) struct Spawner {
    scheduler: Arc<SchedulerPriv>,
}

pub(crate) struct Queues<S: 'static> {
    /// List of all active tasks spawned onto this executor.
    ///
    /// # Safety
    ///
    /// Must only be accessed from the primary thread
    owned_tasks: UnsafeCell<task::OwnedList<S>>,
    /// Local run queue.
    ///
    /// Tasks notified from the current thread are pushed into this queue.
    ///
    /// # Safety
    ///
    /// References should not be handed out. Only call `push` / `pop` functions.
    /// Only call from the owning thread.
    local_queue: UnsafeCell<VecDeque<Task<S>>>,

    /// Remote run queue.
    ///
    /// Tasks notified from another thread are pushed into this queue.
    remote_queue: Mutex<RemoteQueue<S>>,

    /// Tasks pending drop
    pending_drop: task::TransferStack<S>,
}

/// The scheduler component.
pub(super) struct SchedulerPriv {
    queues: Queues<Self>,
    /// Unpark the blocked thread
    unpark: Box<dyn Unpark>,
}

unsafe impl Send for SchedulerPriv {}
unsafe impl Sync for SchedulerPriv {}

/// Local state
#[derive(Debug)]
struct LocalState<P> {
    /// Current tick
    tick: u8,

    /// Thread park handle
    park: P,
}

pub(crate) struct RemoteQueue<S: 'static> {
    /// FIFO list of tasks
    queue: VecDeque<Task<S>>,

    /// `true` when a task can be pushed into the queue, false otherwise.
    open: bool,
}

/// Max number of tasks to poll per tick.
const MAX_TASKS_PER_TICK: usize = 61;

thread_local! {
    static ACTIVE: Cell<*const SchedulerPriv> = Cell::new(ptr::null())
}

impl<P> BasicScheduler<P>
where
    P: Park,
{
    pub(crate) fn new(park: P) -> BasicScheduler<P> {
        let unpark = park.unpark();

        BasicScheduler {
            scheduler: Arc::new(SchedulerPriv {
                queues: Queues::new(),
                unpark: Box::new(unpark),
            }),
            local: LocalState { tick: 0, park },
        }
    }

    pub(crate) fn spawner(&self) -> Spawner {
        Spawner {
            scheduler: self.scheduler.clone(),
        }
    }

    /// Spawn a future onto the thread pool
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (task, handle) = task::joinable(future);
        self.scheduler.schedule(task);
        handle
    }

    pub(crate) fn block_on<F>(&mut self, mut future: F) -> F::Output
    where
        F: Future,
    {
        use crate::runtime;
        use std::pin::Pin;
        use std::task::Context;
        use std::task::Poll::Ready;

        let local = &mut self.local;
        let scheduler = &*self.scheduler;

        struct Guard {
            old: *const SchedulerPriv,
        }

        impl Drop for Guard {
            fn drop(&mut self) {
                ACTIVE.with(|cell| cell.set(self.old));
            }
        }

        // Track the current scheduler
        let _guard = ACTIVE.with(|cell| {
            let guard = Guard { old: cell.get() };

            cell.set(scheduler as *const SchedulerPriv);

            guard
        });

        runtime::global::with_basic_scheduler(scheduler, || {
            let mut _enter = runtime::enter();

            let raw_waker = RawWaker::new(
                scheduler as *const SchedulerPriv as *const (),
                &RawWakerVTable::new(sched_clone_waker, sched_noop, sched_wake_by_ref, sched_noop),
            );

            let waker = ManuallyDrop::new(unsafe { Waker::from_raw(raw_waker) });
            let mut cx = Context::from_waker(&waker);

            // `block_on` takes ownership of `f`. Once it is pinned here, the
            // original `f` binding can no longer be accessed, making the
            // pinning safe.
            let mut future = unsafe { Pin::new_unchecked(&mut future) };

            loop {
                if let Ready(v) = future.as_mut().poll(&mut cx) {
                    return v;
                }

                scheduler.tick(local);

                // Maintenance work
                unsafe {
                    // safety: this function is safe to call only from the
                    // thread the basic scheduler is running on (which we are).
                    scheduler.queues.drain_pending_drop();
                }
            }
        })
    }
}

impl Spawner {
    /// Spawn a future onto the thread pool
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (task, handle) = task::joinable(future);
        self.scheduler.schedule(task);
        handle
    }

    /// Enter the executor context
    pub(crate) fn enter<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        use crate::runtime::global;
        global::with_basic_scheduler(&*self.scheduler, f)
    }
}

impl<S> Queues<S>
where
    S: Schedule + 'static,
{
    pub(crate) const INITIAL_CAPACITY: usize = 64;

    /// How often to check the remote queue first
    pub(crate) const CHECK_REMOTE_INTERVAL: u8 = 13;

    pub(crate) fn new() -> Self {
        Self {
            owned_tasks: UnsafeCell::new(task::OwnedList::new()),
            local_queue: UnsafeCell::new(VecDeque::with_capacity(Self::INITIAL_CAPACITY)),
            pending_drop: task::TransferStack::new(),
            remote_queue: Mutex::new(RemoteQueue {
                queue: VecDeque::with_capacity(Self::INITIAL_CAPACITY),
                open: true,
            }),
        }
    }

    /// Add a new task to the scheduler.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn add_task(&self, task: &Task<S>) {
        (*self.owned_tasks.get()).insert(task);
    }

    /// Push a task to the local queue.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn push_local(&self, task: Task<S>) {
        (*self.local_queue.get()).push_back(task);
    }

    /// Remove a task from the local queue.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn release_local(&self, task: &Task<S>) {
        (*self.owned_tasks.get()).remove(task);
    }

    /// Lock the remote queue, returning a `MutexGuard`.
    ///
    /// This can be used to push to the remote queue and perform other
    /// operations while holding the lock.
    ///
    /// # Panics
    ///
    /// If the remote queue mutex is poisoned.
    pub(crate) fn remote(&self) -> MutexGuard<'_, RemoteQueue<S>> {
        self.remote_queue
            .lock()
            .expect("failed to lock remote queue")
    }

    /// Release a task from outside of the thread that owns the scheduler.
    ///
    /// This simply pushes the task to the pending drop queue.
    pub(crate) fn release_remote(&self, task: Task<S>) {
        self.pending_drop.push(task);
    }

    /// Returns the next task from the remote *or* local queue.
    ///
    /// Typically, this checks the local queue before the remote queue, and only
    /// checks the remote queue if the local queue is empty. However, to avoid
    /// starving the remote queue, it is checked first every
    /// `CHECK_REMOTE_INTERVAL` ticks.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn next_task(&self, tick: u8) -> Option<Task<S>> {
        if 0 == tick % Self::CHECK_REMOTE_INTERVAL {
            self.next_remote_task().or_else(|| self.next_local_task())
        } else {
            self.next_local_task().or_else(|| self.next_remote_task())
        }
    }

    /// Returns the next task from the local queue.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn next_local_task(&self) -> Option<Task<S>> {
        (*self.local_queue.get()).pop_front()
    }

    /// Returns the next task from the remote queue.
    ///
    /// # Panics
    ///
    /// If the mutex around the remote queue is poisoned _and_ the current
    /// thread is not already panicking. This is safe to call in a `Drop` impl.
    pub(crate) fn next_remote_task(&self) -> Option<Task<S>> {
        // there is no semantic information in the `PoisonError`, and it
        // doesn't implement `Debug`, but clippy thinks that it's bad to
        // match all errors here...
        #[allow(clippy::match_wild_err_arm)]
        let mut lock = match self.remote_queue.lock() {
            // If the lock is poisoned, but the thread is already panicking,
            // avoid a double panic. This is necessary since `next_task` (which
            // calls `next_remote_task`) can be called in the `Drop` impl.
            Err(_) if std::thread::panicking() => return None,
            Err(_) => panic!("mutex poisoned"),
            Ok(lock) => lock,
        };
        lock.queue.pop_front()
    }

    /// Returns true if any owned tasks are still bound to this scheduler.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn has_tasks_remaining(&self) -> bool {
        !(*self.owned_tasks.get()).is_empty()
    }

    /// Drain any tasks that have previously been released from other threads.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    pub(crate) unsafe fn drain_pending_drop(&self) {
        for task in self.pending_drop.drain() {
            (*self.owned_tasks.get()).remove(&task);
            drop(task);
        }
    }

    /// Shut down the queues.
    ///
    /// This performs the following operations:
    ///
    /// 1. Close the remote queue (so that it will no longer accept new tasks).
    /// 2. Drain the remote queue and shut down all tasks.
    /// 3. Drain the local queue and shut down all tasks.
    /// 4. Shut down the owned task list.
    /// 5. Drain the list of tasks dropped externally and remove them from the
    ///    owned task list.
    ///
    /// This method should be called before dropping a `Queues`. It is provided
    /// as a method rather than a `Drop` impl because types that own a `Queues`
    /// wish to perform other work in their `Drop` implementations _after_
    /// shutting down the task queues.
    ///
    /// # Safety
    ///
    /// This method accesses the local task queue, and therefore *must* be
    /// called only from the thread that owns the scheduler.
    ///
    /// # Panics
    ///
    /// If the mutex around the remote queue is poisoned _and_ the current
    /// thread is not already panicking. This is safe to call in a `Drop` impl.
    pub(crate) unsafe fn shutdown(&self) {
        // Close and drain the remote queue.
        self.close_remote();

        // Drain the local queue.
        self.close_local();

        // Release owned tasks
        self.shutdown_owned_tasks();

        // Drain tasks pending drop.
        self.drain_pending_drop();
    }

    /// Shut down the scheduler's owned task list.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    unsafe fn shutdown_owned_tasks(&self) {
        (*self.owned_tasks.get()).shutdown();
    }

    /// Drain the remote queue, and shut down its tasks.
    ///
    /// This closes the remote queue. Any additional tasks added to it will be
    /// shut down instead.
    ///
    /// # Panics
    /// If the mutex around the remote queue is poisoned _and_ the current
    /// thread is not already panicking. This is safe to call in a `Drop` impl.
    fn close_remote(&self) {
        #[allow(clippy::match_wild_err_arm)]
        let mut lock = match self.remote_queue.lock() {
            // If the lock is poisoned, but the thread is already panicking,
            // avoid a double panic. This is necessary since this fn can be
            // called in a drop impl.
            Err(_) if std::thread::panicking() => return,
            Err(_) => panic!("mutex poisoned"),
            Ok(lock) => lock,
        };
        lock.open = false;

        while let Some(task) = lock.queue.pop_front() {
            task.shutdown();
        }
    }

    /// Drain the local queue, and shut down its tasks.
    ///
    /// # Safety
    ///
    /// This *must* be called only from the thread that owns the scheduler.
    unsafe fn close_local(&self) {
        while let Some(task) = self.next_local_task() {
            task.shutdown();
        }
    }
}

impl<S> fmt::Debug for Queues<S> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Queues")
            .field("owned_tasks", &self.owned_tasks)
            .field("remote_queue", &self.remote_queue)
            .field("local_queue", &self.local_queue)
            .finish()
    }
}

// === impl RemoteQueue ===

impl<S> RemoteQueue<S>
where
    S: Schedule,
{
    /// Schedule a remote task.
    ///
    /// If the queue is open to accept new tasks, the task is pushed to the back
    /// of the queue. Otherwise, if the queue is closed (the scheduler is
    /// shutting down), the new task will be shut down immediately.
    pub(crate) fn schedule(&mut self, task: Task<S>) {
        if self.open {
            self.queue.push_back(task);
        } else {
            task.shutdown();
        }
    }
}

impl<S> fmt::Debug for RemoteQueue<S> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("RemoteQueue")
            .field("queue", &self.queue)
            .field("open", &self.open)
            .finish()
    }
}

// === impl SchedulerPriv ===

impl SchedulerPriv {
    fn tick(&self, local: &mut LocalState<impl Park>) {
        for _ in 0..MAX_TASKS_PER_TICK {
            // Get the current tick
            let tick = local.tick;

            // Increment the tick
            local.tick = tick.wrapping_add(1);
            let next = unsafe {
                // safety: this function is safe to call only from the
                // thread the basic scheduler is running on. The `LocalState`
                // parameter to this method implies that we are on that thread.
                self.queues.next_task(tick)
            };

            let task = match next {
                Some(task) => task,
                None => {
                    local.park.park().ok().expect("failed to park");
                    return;
                }
            };

            if let Some(task) = task.run(&mut || Some(self.into())) {
                unsafe {
                    // safety: this function is safe to call only from the
                    // thread the basic scheduler is running on. The `LocalState`
                    // parameter to this method implies that we are on that thread.
                    self.queues.push_local(task);
                }
            }
        }

        local
            .park
            .park_timeout(Duration::from_millis(0))
            .ok()
            .expect("failed to park");
    }

    /// # Safety
    ///
    /// Must be called from the same thread that holds the `BasicScheduler`
    /// value.
    pub(super) unsafe fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (task, handle) = task::joinable(future);
        self.queues.push_local(task);
        handle
    }
}

impl Schedule for SchedulerPriv {
    fn bind(&self, task: &Task<Self>) {
        unsafe {
            // safety: `Queues::add_task` is only safe to call from the thread
            // that owns the queues (the thread the scheduler is running on).
            // `Scheduler::bind` is called when polling a task that
            // doesn't have a scheduler set. We will only poll tasks from the
            // thread that the scheduler is running on. Therefore, this is safe
            // to call.
            self.queues.add_task(task);
        }
    }

    fn release(&self, task: Task<Self>) {
        self.queues.release_remote(task);
    }

    fn release_local(&self, task: &Task<Self>) {
        unsafe {
            // safety: `Scheduler::release_local` is only called from the
            // thread that the scheduler is running on. The `Schedule` trait's
            // contract is that releasing a task from another thread should call
            // `release` rather than `release_local`.
            self.queues.release_local(task);
        }
    }

    fn schedule(&self, task: Task<Self>) {
        let is_current = ACTIVE.with(|cell| cell.get() == self as *const SchedulerPriv);

        if is_current {
            unsafe {
                // safety: this function is safe to call only from the
                // thread the basic scheduler is running on. If `is_current` is
                // then we are on that thread.
                self.queues.push_local(task)
            };
        } else {
            let mut lock = self.queues.remote();
            lock.schedule(task);

            // while locked, call unpark
            self.unpark.unpark();

            drop(lock);
        }
    }
}

impl ScheduleSendOnly for SchedulerPriv {}

impl<P> Drop for BasicScheduler<P>
where
    P: Park,
{
    fn drop(&mut self) {
        unsafe {
            // safety: the `Drop` impl owns the scheduler's queues. these fields
            // will only be accessed when running the scheduler, and it can no
            // longer be run, since we are in the process of dropping it.

            // Shut down the task queues.
            self.scheduler.queues.shutdown();
        }

        // Wait until all tasks have been released.
        while unsafe { self.scheduler.queues.has_tasks_remaining() } {
            self.local.park.park().ok().expect("park failed");
            unsafe {
                self.scheduler.queues.drain_pending_drop();
            }
        }
    }
}

impl fmt::Debug for SchedulerPriv {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Scheduler")
            .field("queues", &self.queues)
            .finish()
    }
}

unsafe fn sched_clone_waker(ptr: *const ()) -> RawWaker {
    let s1 = ManuallyDrop::new(Arc::from_raw(ptr as *const SchedulerPriv));

    #[allow(clippy::redundant_clone)]
    let s2 = s1.clone();

    RawWaker::new(
        &**s2 as *const SchedulerPriv as *const (),
        &RawWakerVTable::new(sched_clone_waker, sched_wake, sched_wake_by_ref, sched_drop),
    )
}

unsafe fn sched_wake(ptr: *const ()) {
    let scheduler = Arc::from_raw(ptr as *const SchedulerPriv);
    scheduler.unpark.unpark();
}

unsafe fn sched_wake_by_ref(ptr: *const ()) {
    let scheduler = ManuallyDrop::new(Arc::from_raw(ptr as *const SchedulerPriv));
    scheduler.unpark.unpark();
}

unsafe fn sched_drop(ptr: *const ()) {
    let _ = Arc::from_raw(ptr as *const SchedulerPriv);
}

unsafe fn sched_noop(_ptr: *const ()) {
    unreachable!();
}
