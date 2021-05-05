use crate::future::poll_fn;
use crate::loom::sync::atomic::AtomicBool;
use crate::loom::sync::Mutex;
use crate::park::{Park, Unpark};
use crate::runtime::task::{self, JoinHandle, Schedule, Task};
use crate::sync::notify::Notify;
use crate::util::linked_list::{Link, LinkedList};
use crate::util::{waker_ref, Wake, WakerRef};

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};
use std::sync::Arc;
use std::task::Poll::{Pending, Ready};
use std::time::Duration;

/// Executes tasks on the current thread
pub(crate) struct BasicScheduler<P: Park> {
    /// Inner state guarded by a mutex that is shared
    /// between all `block_on` calls.
    inner: Mutex<Option<Inner<P>>>,

    /// Notifier for waking up other threads to steal the
    /// parker.
    notify: Notify,

    /// Sendable task spawner
    spawner: Spawner,
}

/// The inner scheduler that owns the task queue and the main parker P.
struct Inner<P: Park> {
    /// Scheduler run queue
    ///
    /// When the scheduler is executed, the queue is removed from `self` and
    /// moved into `Context`.
    ///
    /// This indirection is to allow `BasicScheduler` to be `Send`.
    tasks: Option<Tasks>,

    /// Sendable task spawner
    spawner: Spawner,

    /// Current tick
    tick: u8,

    /// Thread park handle
    park: P,
}

#[derive(Clone)]
pub(crate) struct Spawner {
    shared: Arc<Shared>,
}

struct Tasks {
    /// Collection of all active tasks spawned onto this executor.
    owned: LinkedList<Task<Arc<Shared>>, <Task<Arc<Shared>> as Link>::Target>,

    /// Local run queue.
    ///
    /// Tasks notified from the current thread are pushed into this queue.
    queue: VecDeque<task::Notified<Arc<Shared>>>,
}

/// A remote scheduler entry.
///
/// These are filled in by remote threads sending instructions to the scheduler.
enum Entry {
    /// A remote thread wants to spawn a task.
    Schedule(task::Notified<Arc<Shared>>),
    /// A remote thread wants a task to be released by the scheduler. We only
    /// have access to its header.
    Release(NonNull<task::Header>),
}

// Safety: Used correctly, the task header is "thread safe". Ultimately the task
// is owned by the current thread executor, for which this instruction is being
// sent.
unsafe impl Send for Entry {}

/// Scheduler state shared between threads.
struct Shared {
    /// Remote run queue
    queue: Mutex<VecDeque<Entry>>,

    /// Unpark the blocked thread
    unpark: Box<dyn Unpark>,

    // indicates whether the blocked on thread was woken
    woken: AtomicBool,
}

/// Thread-local context.
struct Context {
    /// Shared scheduler state
    shared: Arc<Shared>,

    /// Local queue
    tasks: RefCell<Tasks>,
}

/// Initial queue capacity.
const INITIAL_CAPACITY: usize = 64;

/// Max number of tasks to poll per tick.
#[cfg(loom)]
const MAX_TASKS_PER_TICK: usize = 4;
#[cfg(not(loom))]
const MAX_TASKS_PER_TICK: usize = 61;

/// How often to check the remote queue first.
const REMOTE_FIRST_INTERVAL: u8 = 31;

// Tracks the current BasicScheduler.
scoped_thread_local!(static CURRENT: Context);

impl<P: Park> BasicScheduler<P> {
    pub(crate) fn new(park: P) -> BasicScheduler<P> {
        let unpark = Box::new(park.unpark());

        let spawner = Spawner {
            shared: Arc::new(Shared {
                queue: Mutex::new(VecDeque::with_capacity(INITIAL_CAPACITY)),
                unpark: unpark as Box<dyn Unpark>,
                woken: AtomicBool::new(false),
            }),
        };

        let inner = Mutex::new(Some(Inner {
            tasks: Some(Tasks {
                owned: LinkedList::new(),
                queue: VecDeque::with_capacity(INITIAL_CAPACITY),
            }),
            spawner: spawner.clone(),
            tick: 0,
            park,
        }));

        BasicScheduler {
            inner,
            notify: Notify::new(),
            spawner,
        }
    }

    pub(crate) fn spawner(&self) -> &Spawner {
        &self.spawner
    }

    pub(crate) fn block_on<F: Future>(&self, future: F) -> F::Output {
        pin!(future);

        // Attempt to steal the dedicated parker and block_on the future if we can there,
        // otherwise, lets select on a notification that the parker is available
        // or the future is complete.
        loop {
            if let Some(inner) = &mut self.take_inner() {
                return inner.block_on(future);
            } else {
                let mut enter = crate::runtime::enter(false);

                let notified = self.notify.notified();
                pin!(notified);

                if let Some(out) = enter
                    .block_on(poll_fn(|cx| {
                        if notified.as_mut().poll(cx).is_ready() {
                            return Ready(None);
                        }

                        if let Ready(out) = future.as_mut().poll(cx) {
                            return Ready(Some(out));
                        }

                        Pending
                    }))
                    .expect("Failed to `Enter::block_on`")
                {
                    return out;
                }
            }
        }
    }

    fn take_inner(&self) -> Option<InnerGuard<'_, P>> {
        let inner = self.inner.lock().take()?;

        Some(InnerGuard {
            inner: Some(inner),
            basic_scheduler: &self,
        })
    }
}

impl<P: Park> Inner<P> {
    /// Block on the future provided and drive the runtime's driver.
    fn block_on<F: Future>(&mut self, future: F) -> F::Output {
        enter(self, |scheduler, context| {
            let _enter = crate::runtime::enter(false);
            let waker = scheduler.spawner.waker_ref();
            let mut cx = std::task::Context::from_waker(&waker);
            let mut polled = false;

            pin!(future);

            'outer: loop {
                if scheduler.spawner.was_woken() || !polled {
                    polled = true;
                    if let Ready(v) = crate::coop::budget(|| future.as_mut().poll(&mut cx)) {
                        return v;
                    }
                }

                for _ in 0..MAX_TASKS_PER_TICK {
                    // Get and increment the current tick
                    let tick = scheduler.tick;
                    scheduler.tick = scheduler.tick.wrapping_add(1);

                    let entry = if tick % REMOTE_FIRST_INTERVAL == 0 {
                        scheduler.spawner.pop().or_else(|| {
                            context
                                .tasks
                                .borrow_mut()
                                .queue
                                .pop_front()
                                .map(Entry::Schedule)
                        })
                    } else {
                        context
                            .tasks
                            .borrow_mut()
                            .queue
                            .pop_front()
                            .map(Entry::Schedule)
                            .or_else(|| scheduler.spawner.pop())
                    };

                    let entry = match entry {
                        Some(entry) => entry,
                        None => {
                            // Park until the thread is signaled
                            scheduler.park.park().expect("failed to park");

                            // Try polling the `block_on` future next
                            continue 'outer;
                        }
                    };

                    match entry {
                        Entry::Schedule(task) => crate::coop::budget(|| task.run()),
                        Entry::Release(ptr) => {
                            // Safety: the task header is only legally provided
                            // internally in the header, so we know that it is a
                            // valid (or in particular *allocated*) header that
                            // is part of the linked list.
                            unsafe {
                                let removed = context.tasks.borrow_mut().owned.remove(ptr);

                                // TODO: This seems like it should hold, because
                                // there doesn't seem to be an avenue for anyone
                                // else to fiddle with the owned tasks
                                // collection *after* a remote thread has marked
                                // it as released, and at that point, the only
                                // location at which it can be removed is here
                                // or in the Drop implementation of the
                                // scheduler.
                                debug_assert!(removed.is_some());
                            }
                        }
                    }
                }

                // Yield to the park, this drives the timer and pulls any pending
                // I/O events.
                scheduler
                    .park
                    .park_timeout(Duration::from_millis(0))
                    .expect("failed to park");
            }
        })
    }
}

/// Enter the scheduler context. This sets the queue and other necessary
/// scheduler state in the thread-local
fn enter<F, R, P>(scheduler: &mut Inner<P>, f: F) -> R
where
    F: FnOnce(&mut Inner<P>, &Context) -> R,
    P: Park,
{
    // Ensures the run queue is placed back in the `BasicScheduler` instance
    // once `block_on` returns.`
    struct Guard<'a, P: Park> {
        context: Option<Context>,
        scheduler: &'a mut Inner<P>,
    }

    impl<P: Park> Drop for Guard<'_, P> {
        fn drop(&mut self) {
            let Context { tasks, .. } = self.context.take().expect("context missing");
            self.scheduler.tasks = Some(tasks.into_inner());
        }
    }

    // Remove `tasks` from `self` and place it in a `Context`.
    let tasks = scheduler.tasks.take().expect("invalid state");

    let guard = Guard {
        context: Some(Context {
            shared: scheduler.spawner.shared.clone(),
            tasks: RefCell::new(tasks),
        }),
        scheduler,
    };

    let context = guard.context.as_ref().unwrap();
    let scheduler = &mut *guard.scheduler;

    CURRENT.set(context, || f(scheduler, context))
}

impl<P: Park> Drop for BasicScheduler<P> {
    fn drop(&mut self) {
        // Avoid a double panic if we are currently panicking and
        // the lock may be poisoned.

        let mut inner = match self.inner.lock().take() {
            Some(inner) => inner,
            None if std::thread::panicking() => return,
            None => panic!("Oh no! We never placed the Inner state back, this is a bug!"),
        };

        enter(&mut inner, |scheduler, context| {
            // Loop required here to ensure borrow is dropped between iterations
            #[allow(clippy::while_let_loop)]
            loop {
                let task = match context.tasks.borrow_mut().owned.pop_back() {
                    Some(task) => task,
                    None => break,
                };

                task.shutdown();
            }

            // Drain local queue
            for task in context.tasks.borrow_mut().queue.drain(..) {
                task.shutdown();
            }

            // Drain remote queue
            for entry in scheduler.spawner.shared.queue.lock().drain(..) {
                match entry {
                    Entry::Schedule(task) => {
                        task.shutdown();
                    }
                    Entry::Release(..) => {
                        // Do nothing, each entry in the linked list was *just*
                        // dropped by the scheduler above.
                    }
                }
            }

            assert!(context.tasks.borrow().owned.is_empty());
        });
    }
}

impl<P: Park> fmt::Debug for BasicScheduler<P> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("BasicScheduler").finish()
    }
}

// ===== impl Spawner =====

impl Spawner {
    /// Spawns a future onto the thread pool
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (task, handle) = task::joinable(future);
        self.shared.schedule(task);
        handle
    }

    fn pop(&self) -> Option<Entry> {
        self.shared.queue.lock().pop_front()
    }

    fn waker_ref(&self) -> WakerRef<'_> {
        // clear the woken bit
        self.shared.woken.swap(false, AcqRel);
        waker_ref(&self.shared)
    }

    fn was_woken(&self) -> bool {
        self.shared.woken.load(Acquire)
    }
}

impl fmt::Debug for Spawner {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Spawner").finish()
    }
}

// ===== impl Shared =====

impl Schedule for Arc<Shared> {
    fn bind(task: Task<Self>) -> Arc<Shared> {
        CURRENT.with(|maybe_cx| {
            let cx = maybe_cx.expect("scheduler context missing");
            cx.tasks.borrow_mut().owned.push_front(task);
            cx.shared.clone()
        })
    }

    fn release(&self, task: &Task<Self>) -> Option<Task<Self>> {
        CURRENT.with(|maybe_cx| {
            let ptr = NonNull::from(task.header());

            if let Some(cx) = maybe_cx {
                // safety: the task is inserted in the list in `bind`.
                unsafe { cx.tasks.borrow_mut().owned.remove(ptr) }
            } else {
                self.queue.lock().push_back(Entry::Release(ptr));
                self.unpark.unpark();
                // Returning `None` here prevents the task plumbing from being
                // freed. It is then up to the scheduler through the queue we
                // just added to, or its Drop impl to free the task.
                None
            }
        })
    }

    fn schedule(&self, task: task::Notified<Self>) {
        CURRENT.with(|maybe_cx| match maybe_cx {
            Some(cx) if Arc::ptr_eq(self, &cx.shared) => {
                cx.tasks.borrow_mut().queue.push_back(task);
            }
            _ => {
                self.queue.lock().push_back(Entry::Schedule(task));
                self.unpark.unpark();
            }
        });
    }
}

impl Wake for Shared {
    fn wake(self: Arc<Self>) {
        Wake::wake_by_ref(&self)
    }

    /// Wake by reference
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.woken.store(true, Release);
        arc_self.unpark.unpark();
    }
}

// ===== InnerGuard =====

/// Used to ensure we always place the Inner value
/// back into its slot in `BasicScheduler`, even if the
/// future panics.
struct InnerGuard<'a, P: Park> {
    inner: Option<Inner<P>>,
    basic_scheduler: &'a BasicScheduler<P>,
}

impl<P: Park> InnerGuard<'_, P> {
    fn block_on<F: Future>(&mut self, future: F) -> F::Output {
        // The only time inner gets set to `None` is if we have dropped
        // already so this unwrap is safe.
        self.inner.as_mut().unwrap().block_on(future)
    }
}

impl<P: Park> Drop for InnerGuard<'_, P> {
    fn drop(&mut self) {
        if let Some(scheduler) = self.inner.take() {
            let mut lock = self.basic_scheduler.inner.lock();

            // Replace old scheduler back into the state to allow
            // other threads to pick it up and drive it.
            lock.replace(scheduler);

            // Wake up other possible threads that could steal
            // the dedicated parker P.
            self.basic_scheduler.notify.notify_one()
        }
    }
}
