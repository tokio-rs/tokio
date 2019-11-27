use crate::park::{Park, Unpark};
use crate::task::{self, JoinHandle, Schedule, ScheduleSendOnly, Task};

use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
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

/// The scheduler component.
pub(super) struct SchedulerPriv {
    /// List of all active tasks spawned onto this executor.
    ///
    /// # Safety
    ///
    /// Must only be accessed from the primary thread
    owned_tasks: UnsafeCell<task::OwnedList<Self>>,

    /// Local run queue.
    ///
    /// Tasks notified from the current thread are pushed into this queue.
    ///
    /// # Safety
    ///
    /// References should not be handed out. Only call `push` / `pop` functions.
    /// Only call from the owning thread.
    local_queue: UnsafeCell<VecDeque<Task<SchedulerPriv>>>,

    /// Remote run queue.
    ///
    /// Tasks notified from another thread are pushed into this queue.
    remote_queue: Mutex<RemoteQueue>,

    /// Tasks pending drop
    pending_drop: task::TransferStack<Self>,

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

#[derive(Debug)]
struct RemoteQueue {
    /// FIFO list of tasks
    queue: VecDeque<Task<SchedulerPriv>>,

    /// `true` when a task can be pushed into the queue, false otherwise.
    open: bool,
}

/// Max number of tasks to poll per tick.
const MAX_TASKS_PER_TICK: usize = 61;

/// How often to check the remote queue first
const CHECK_REMOTE_INTERVAL: u8 = 13;

impl<P> BasicScheduler<P>
where
    P: Park,
{
    pub(crate) fn new(park: P) -> BasicScheduler<P> {
        let unpark = park.unpark();

        BasicScheduler {
            scheduler: Arc::new(SchedulerPriv {
                owned_tasks: UnsafeCell::new(task::OwnedList::new()),
                local_queue: UnsafeCell::new(VecDeque::with_capacity(64)),
                remote_queue: Mutex::new(RemoteQueue {
                    queue: VecDeque::with_capacity(64),
                    open: true,
                }),
                pending_drop: task::TransferStack::new(),
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
                scheduler.drain_pending_drop();
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

impl SchedulerPriv {
    fn tick(&self, local: &mut LocalState<impl Park>) {
        for _ in 0..MAX_TASKS_PER_TICK {
            // Get the current tick
            let tick = local.tick;

            // Increment the tick
            local.tick = tick.wrapping_add(1);

            let task = match self.next_task(tick) {
                Some(task) => task,
                None => {
                    local.park.park().ok().expect("failed to park");
                    return;
                }
            };

            if let Some(task) = task.run(&mut || Some(self.into())) {
                unsafe {
                    self.schedule_local(task);
                }
            }
        }

        local
            .park
            .park_timeout(Duration::from_millis(0))
            .ok()
            .expect("failed to park");
    }

    fn drain_pending_drop(&self) {
        for task in self.pending_drop.drain() {
            unsafe {
                (*self.owned_tasks.get()).remove(&task);
            }
            drop(task);
        }
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
        self.schedule_local(task);
        handle
    }

    unsafe fn schedule_local(&self, task: Task<Self>) {
        (*self.local_queue.get()).push_back(task);
    }

    fn next_task(&self, tick: u8) -> Option<Task<Self>> {
        if 0 == tick % CHECK_REMOTE_INTERVAL {
            self.next_remote_task().or_else(|| self.next_local_task())
        } else {
            self.next_local_task().or_else(|| self.next_remote_task())
        }
    }

    fn next_local_task(&self) -> Option<Task<Self>> {
        unsafe { (*self.local_queue.get()).pop_front() }
    }

    fn next_remote_task(&self) -> Option<Task<Self>> {
        self.remote_queue.lock().unwrap().queue.pop_front()
    }
}

impl Schedule for SchedulerPriv {
    fn bind(&self, task: &Task<Self>) {
        unsafe {
            (*self.owned_tasks.get()).insert(task);
        }
    }

    fn release(&self, task: Task<Self>) {
        self.pending_drop.push(task);
    }

    fn release_local(&self, task: &Task<Self>) {
        unsafe {
            (*self.owned_tasks.get()).remove(task);
        }
    }

    fn schedule(&self, task: Task<Self>) {
        use crate::runtime::global;

        if global::basic_scheduler_is_current(self) {
            unsafe { self.schedule_local(task) };
        } else {
            let mut lock = self.remote_queue.lock().unwrap();

            if lock.open {
                lock.queue.push_back(task);
            } else {
                task.shutdown();
            }

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
        // Close the remote queue
        let mut lock = self.scheduler.remote_queue.lock().unwrap();
        lock.open = false;

        while let Some(task) = lock.queue.pop_front() {
            task.shutdown();
        }

        drop(lock);

        // Drain all local tasks
        while let Some(task) = self.scheduler.next_local_task() {
            task.shutdown();
        }

        // Release owned tasks
        unsafe {
            (*self.scheduler.owned_tasks.get()).shutdown();
        }

        self.scheduler.drain_pending_drop();

        // Wait until all tasks have been released.
        while unsafe { !(*self.scheduler.owned_tasks.get()).is_empty() } {
            self.local.park.park().ok().expect("park failed");
            self.scheduler.drain_pending_drop();
        }
    }
}

impl fmt::Debug for SchedulerPriv {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Scheduler").finish()
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
