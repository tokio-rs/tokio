use crate::loom;
use crate::loom::sync::{Condvar, Mutex};
use crate::park::{Park, ParkThread, Unpark};
use crate::runtime;
use crate::runtime::task::{self, JoinHandle, Schedule, Task};
use crate::util::linked_list::LinkedList;
use crate::util::{waker_ref, Wake, WakerRef};

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::task::Poll::Ready;
use std::time::Duration;

/// Executes tasks on the current thread
pub(crate) struct BasicScheduler<P: Park> {
    /// Inner with the dedicated parker P
    inner: Mutex<Option<Inner<P>>>,

    /// Sync items used to notify other threads that called
    /// block_on concurrently that the dedicated driver is available
    /// to steal
    condvar: loom::sync::Arc<Condvar>,
    mutex: loom::sync::Arc<Mutex<()>>,

    /// Sendable task spawner
    spawner: Spawner,
}

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
    owned: LinkedList<Task<Arc<Shared>>>,

    /// Local run queue.
    ///
    /// Tasks notified from the current thread are pushed into this queue.
    queue: VecDeque<task::Notified<Arc<Shared>>>,
}

/// Scheduler state shared between threads.
struct Shared {
    /// Remote run queue
    queue: Mutex<VecDeque<task::Notified<Arc<Shared>>>>,

    /// Unpark the blocked thread
    unpark: Box<dyn Unpark>,
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
const MAX_TASKS_PER_TICK: usize = 61;

/// How often to check the remote queue first.
const REMOTE_FIRST_INTERVAL: u8 = 31;

// Tracks the current BasicScheduler.
scoped_thread_local!(static CURRENT: Context);

impl<P> BasicScheduler<P>
where
    P: Park,
{
    pub(crate) fn new(park: P) -> BasicScheduler<P> {
        let unpark = Box::new(park.unpark());

        let spawner = Spawner {
            shared: Arc::new(Shared {
                queue: Mutex::new(VecDeque::with_capacity(INITIAL_CAPACITY)),
                unpark: unpark as Box<dyn Unpark>,
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
            spawner,
            condvar: loom::sync::Arc::new(Condvar::new()),
            mutex: loom::sync::Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn spawner(&self) -> &Spawner {
        &self.spawner
    }

    /// Spawns a future onto the thread pool
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.spawner.spawn(future)
    }

    pub(crate) fn block_on<F: Future>(&self, future: F) -> F::Output {
        // If we can steal the dedicated parker than lets block_on that
        // otherwise, lets block_on and attempt to steal it back if we can.
        if let Some(mut inner) = self.take_inner() {
            inner.block_on(future)
        } else {
            // TODO: should this be false or true? In the origina block_on for
            // basic_scheduler we have false?
            let enter = crate::runtime::enter(false);

            let mut park = ParkThread::with_condvar(self.condvar.clone(), self.mutex.clone());
            let waker = park.unpark().into_waker();
            let mut cx = std::task::Context::from_waker(&waker);

            pin!(future);

            loop {
                if let Ready(v) = crate::coop::budget(|| future.as_mut().poll(&mut cx)) {
                    return v;
                }

                // Check if we can steal the driver
                // TODO: Consider using an atomic load here intead of locking
                // the mutex.
                if let Some(mut inner) = self.take_inner() {
                    // We will enter again on in the inner implementation below
                    drop(enter);
                    return inner.block_on(future);
                }

                // Park this thread waiting for some external wake up from
                // a waker or a notification that we can steal the driver again.
                park.park().expect("failed to park");
            }
        }
    }

    fn take_inner(&self) -> Option<InnerGuard<'_, P>> {
        let mut lock = self.inner.lock().unwrap();
        let inner = lock.take()?;

        Some(InnerGuard {
            inner: Some(inner),
            scheduler: &self,
        })
    }
}

impl<P: Park> Inner<P> {
    /// Block on the future provided and drive the runtime's driver.
    fn block_on<F: Future>(&mut self, future: F) -> F::Output {
        enter(self, |scheduler, context| {
            let _enter = runtime::enter(false);
            let waker = scheduler.spawner.waker_ref();
            let mut cx = std::task::Context::from_waker(&waker);

            pin!(future);

            'outer: loop {
                if let Ready(v) = crate::coop::budget(|| future.as_mut().poll(&mut cx)) {
                    return v;
                }

                for _ in 0..MAX_TASKS_PER_TICK {
                    // Get and increment the current tick
                    let tick = scheduler.tick;
                    scheduler.tick = scheduler.tick.wrapping_add(1);

                    let next = if tick % REMOTE_FIRST_INTERVAL == 0 {
                        scheduler
                            .spawner
                            .pop()
                            .or_else(|| context.tasks.borrow_mut().queue.pop_front())
                    } else {
                        context
                            .tasks
                            .borrow_mut()
                            .queue
                            .pop_front()
                            .or_else(|| scheduler.spawner.pop())
                    };

                    match next {
                        Some(task) => crate::coop::budget(|| task.run()),
                        None => {
                            // Park until the thread is signaled
                            scheduler.park.park().ok().expect("failed to park");

                            // Try polling the `block_on` future next
                            continue 'outer;
                        }
                    }
                }

                // Yield to the park, this drives the timer and pulls any pending
                // I/O events.
                scheduler
                    .park
                    .park_timeout(Duration::from_millis(0))
                    .ok()
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
        let mut inner = {
            let mut lock = self.inner.lock().expect("BasicScheduler Inner lock");
            lock.take()
                .expect("Oh no! We never placed the Inner state back!")
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
            for task in scheduler.spawner.shared.queue.lock().unwrap().drain(..) {
                task.shutdown();
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

    fn pop(&self) -> Option<task::Notified<Arc<Shared>>> {
        self.shared.queue.lock().unwrap().pop_front()
    }

    fn waker_ref(&self) -> WakerRef<'_> {
        waker_ref(&self.shared)
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
        use std::ptr::NonNull;

        CURRENT.with(|maybe_cx| {
            let cx = maybe_cx.expect("scheduler context missing");

            // safety: the task is inserted in the list in `bind`.
            unsafe {
                let ptr = NonNull::from(task.header());
                cx.tasks.borrow_mut().owned.remove(ptr)
            }
        })
    }

    fn schedule(&self, task: task::Notified<Self>) {
        CURRENT.with(|maybe_cx| match maybe_cx {
            Some(cx) if Arc::ptr_eq(self, &cx.shared) => {
                cx.tasks.borrow_mut().queue.push_back(task);
            }
            _ => {
                self.queue.lock().unwrap().push_back(task);
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
        arc_self.unpark.unpark();
    }
}

// ===== InnerGuard =====

/// Used to ensure we always place the Inner value
/// back into its slot in `BasicScheduler` even if the
/// future panics.
struct InnerGuard<'a, P: Park> {
    inner: Option<Inner<P>>,
    scheduler: &'a BasicScheduler<P>,
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
        if let Some(inner) = self.inner.take() {
            let mut lock = self.scheduler.inner.lock().unwrap();
            lock.replace(inner);

            // Wake up possible other threads
            // notifying them that they might need
            // to steal the driver.
            drop(self.scheduler.mutex.lock().unwrap());
            self.scheduler.condvar.notify_one();
        }
    }
}
