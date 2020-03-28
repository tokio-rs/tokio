use crate::park::{Park, Unpark};
use crate::runtime;
use crate::runtime::task::{self, JoinHandle, Schedule, Task};
use crate::util::linked_list::LinkedList;
use crate::util::{waker_ref, Wake};

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::Poll::Ready;
use std::time::Duration;

/// Executes tasks on the current thread
pub(crate) struct BasicScheduler<P>
where
    P: Park,
{
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

/// Thread-local context
struct Context {
    /// Shared scheduler state
    shared: Arc<Shared>,

    /// Local queue
    tasks: RefCell<Tasks>,
}

/// Initial queue capacity
const INITIAL_CAPACITY: usize = 64;

/// Max number of tasks to poll per tick.
const MAX_TASKS_PER_TICK: usize = 61;

/// How often ot check the remote queue first
const REMOTE_FIRST_INTERVAL: u8 = 31;

// Tracks the current BasicScheduler
scoped_thread_local!(static CURRENT: Context);

impl<P> BasicScheduler<P>
where
    P: Park,
{
    pub(crate) fn new(park: P) -> BasicScheduler<P> {
        let unpark = Box::new(park.unpark());

        BasicScheduler {
            tasks: Some(Tasks {
                owned: LinkedList::new(),
                queue: VecDeque::with_capacity(INITIAL_CAPACITY),
            }),
            spawner: Spawner {
                shared: Arc::new(Shared {
                    queue: Mutex::new(VecDeque::with_capacity(INITIAL_CAPACITY)),
                    unpark: unpark as Box<dyn Unpark>,
                }),
            },
            tick: 0,
            park,
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

    pub(crate) fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        enter(self, |scheduler, context| {
            let _enter = runtime::enter();
            let waker = waker_ref(&scheduler.spawner.shared);
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
fn enter<F, R, P>(scheduler: &mut BasicScheduler<P>, f: F) -> R
where
    F: FnOnce(&mut BasicScheduler<P>, &Context) -> R,
    P: Park,
{
    // Ensures the run queue is placed back in the `BasicScheduler` instance
    // once `block_on` returns.`
    struct Guard<'a, P: Park> {
        context: Option<Context>,
        scheduler: &'a mut BasicScheduler<P>,
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

impl<P> Drop for BasicScheduler<P>
where
    P: Park,
{
    fn drop(&mut self) {
        enter(self, |scheduler, context| {
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
