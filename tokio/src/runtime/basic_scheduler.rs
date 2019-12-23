use crate::park::{Park, Unpark};
use crate::task::{self, queue::MpscQueues, JoinHandle, Schedule, ScheduleSendOnly, Task};

use std::cell::Cell;
use std::fmt;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::ptr;
use std::sync::Arc;
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
    queues: MpscQueues<Self>,
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
                queues: MpscQueues::new(),
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
        self.scheduler.schedule(task, true);
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
        self.scheduler.schedule(task, true);
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

    fn schedule(&self, task: Task<Self>, spawn: bool) {
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
            lock.schedule(task, spawn);

            // while locked, call unpark
            self.unpark.unpark();

            drop(lock);
        }
    }
}

impl Schedule for SchedulerPriv {
    fn bind(&self, task: &Task<Self>) {
        unsafe {
            // safety: `Queues::add_task` is only safe to call from the thread
            // that owns the queues (the thread the scheduler is running on).
            // `Scheduler::bind` is called when polling a task that
            // doesn't have a scheduler set. We will only poll new tasks from
            // the thread that the scheduler is running on. Therefore, this is
            // safe to call.
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
        SchedulerPriv::schedule(self, task, false);
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
        loop {
            unsafe {
                self.scheduler.queues.drain_pending_drop();
                self.scheduler.queues.drain_queues();

                if !self.scheduler.queues.has_tasks_remaining() {
                    break;
                }

                self.local.park.park().ok().expect("park failed");
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
