//! Runs `!Send` futures on the current thread.
use crate::task::{self, JoinHandle, Schedule, UnsendTask, Unsendable};

use std::cell::{Cell, UnsafeCell};
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::task::{Context, Poll};

use pin_project::pin_project;

/// A group of tasks which are executed on the same thread.
///
/// These tasks need not implement `Send`; a local task group provides the
/// capacity to execute `!Send` futures.
#[derive(Debug)]
pub struct TaskGroup {
    scheduler: Rc<Scheduler>,
    _not_send_or_sync: PhantomData<*const ()>,
}

struct Scheduler {
    /// List of all active tasks spawned onto this executor.
    ///
    /// # Safety
    ///
    /// Must only be accessed from the primary thread
    tasks: UnsafeCell<task::OwnedList<Scheduler, Unsendable>>,

    /// Local run queue.
    ///
    /// Tasks notified from the current thread are pushed into this queue.
    ///
    /// # Safety
    ///
    /// References should not be handed out. Only call `push` / `pop` functions.
    /// Only call from the owning thread.
    queue: UnsafeCell<VecDeque<UnsendTask<Scheduler>>>,
}

#[pin_project]
struct LocalFuture<F> {
    scheduler: Rc<Scheduler>,
    #[pin]
    future: F,
}

thread_local! {
    static CURRENT_TASK_SET: Cell<Option<NonNull<Scheduler>>> = Cell::new(None);
}

/// Spawns a `!Send` future on the local task group.
///
/// The spawned future will be run on the same thread that called `spawn.`
/// This may only be called from the context of a local task group.
///
/// # Panics
///
/// - This function panics if called outside of a local task group.
///
/// # Examples
///
/// ```rust
/// # use tokio::runtime::Runtime;
/// use std::rc::Rc;
/// use tokio::local;
///
/// let unsend_data = Rc::new("my unsend data...");
///
/// let mut rt = Runtime::new().unwrap();
/// let local_group = local::TaskGroup::new();
///
/// // Run the local task grou[].
/// local_group.block_on(&mut rt, async move {
///     let unsend_data = unsend_data.clone();
///     local::spawn(async move {
///         println!("{}", unsend_data);
///         // ...
///     }).await.unwrap();
/// });
/// ```
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    CURRENT_TASK_SET.with(|current| {
        let current = current
            .get()
            .expect("`local::spawn` called from outside of a local::TaskGroup!");
        unsafe {
            let (task, handle) = task::joinable_unsend(future);
            current.as_ref().schedule(task);
            handle
        }
    })
}

/// Max number of tasks to poll per tick.
const MAX_TASKS_PER_TICK: usize = 61;

impl TaskGroup {
    /// Returns a new local task group.
    pub fn new() -> Self {
        Self {
            scheduler: Rc::new(Scheduler::new()),
            _not_send_or_sync: PhantomData,
        }
    }

    /// Spawns a `!Send` task onto the local task group.
    ///
    /// This task is guaranteed to be run on the current thread.
    ///
    /// Unlike the free function [`local::spawn`], this method may be used to
    /// spawn local tasks when the task group is _not_ running. For example:
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// use tokio::local;
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// let local_group = local::TaskGroup::new();
    ///
    /// // Spawn a future on the local group. This future will be run when
    /// // we call `block_on` to drive the task group.
    /// local_group.spawn(async {
    ///    // ...
    /// });
    ///
    /// // Run the local task group.
    /// local_group.block_on(&mut rt, async move {
    ///     // ...
    /// });
    ///
    /// // When `block_on` finishes, we can spawn _more_ futures, which will
    /// // run in subsequent calls to `block_on`.
    /// local_group.spawn(async {
    ///    // ...
    /// });
    ///
    /// local_group.block_on(&mut rt, async move {
    ///     // ...
    /// });
    /// ```
    /// [`local::spawn`]: fn.spawn.html
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (task, handle) = task::joinable_unsend(future);
        self.scheduler.schedule(task);
        handle
    }

    /// Run a future to completion on the provided runtime, driving any local
    /// futures spawned on this task group on the current thread.
    ///
    /// This runs the given future on the runtime, blocking until it is
    /// complete, and yielding its resolved result. Any tasks or timers which
    /// the future spawns internally will be executed on the runtime. The future
    /// may also call [`local::spawn`] to spawn additional local futures on the
    /// current thread.
    ///
    /// This method should not be called from an asynchronous context.
    ///
    /// # Panics
    ///
    /// This function panics if the executor is at capacity, if the provided
    /// future panics, or if called within an asynchronous execution context.
    ///
    /// # Notes
    ///
    /// Since this function internally calls [`Runtime::block_on`], and drives
    /// futures in the local task group inside that call to `block_on`, the local
    /// futures may not use [in-place blocking]. If a blocking call needs to be
    /// issued from a local task, the [`blocking::run`] API may be used instead.
    ///
    /// For example, this will panic:
    /// ```should_panic
    /// use tokio::runtime::{blocking, Runtime};
    /// use tokio::local;
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// let local = local::TaskGroup::new();
    /// local.block_on(&mut rt, async {
    ///     let join = local::spawn(async {
    ///         let blocking_result = blocking::in_place(|| {
    ///             // ...
    ///         });
    ///         // ...
    ///     });
    ///     join.await.unwrap();
    /// })
    /// ```
    /// This, however, will not panic:
    /// ```
    /// use tokio::runtime::{blocking, Runtime};
    /// use tokio::local;
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// let local = local::TaskGroup::new();
    /// local.block_on(&mut rt, async {
    ///     let join = local::spawn(async {
    ///         let blocking_result = blocking::run(|| {
    ///             // ...
    ///         }).await;
    ///         // ...
    ///     });
    ///     join.await.unwrap();
    /// })
    /// ```
    ///
    /// [`local::spawn`]: fn.spawn.html
    /// [`Runtime::block_on`]: ../struct.Runtime.html#method.block_on
    /// [in-place blocking]: ../blocking/fn.in_place.html
    /// [`blocking::run`]: ../blocking/fn.run.html
    pub fn block_on<F>(&self, rt: &mut crate::runtime::Runtime, future: F) -> F::Output
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let scheduler = self.scheduler.clone();
        rt.block_on(LocalFuture { scheduler, future })
    }
}

impl Default for TaskGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl<F: Future> Future for LocalFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let scheduler = this.scheduler;
        let future = this.future;

        scheduler.with(|| {
            scheduler.tick();

            match future.poll(cx) {
                Poll::Ready(v) => Poll::Ready(v),
                Poll::Pending => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
        })
    }
}

// === impl Scheduler ===

impl Schedule<Unsendable> for Scheduler {
    fn bind(&self, task: &UnsendTask<Self>) {
        unsafe {
            (*self.tasks.get()).insert(task);
        }
    }

    fn release(&self, _: UnsendTask<Self>) {
        unreachable!("tasks should only be completed locally")
    }

    fn release_local(&self, task: &UnsendTask<Self>) {
        unsafe {
            (*self.tasks.get()).remove(task);
        }
    }

    fn schedule(&self, task: UnsendTask<Self>) {
        unsafe {
            (*self.queue.get()).push_front(task);
        }
    }
}

impl Scheduler {
    fn new() -> Self {
        Self {
            tasks: UnsafeCell::new(task::OwnedList::new()),
            queue: UnsafeCell::new(VecDeque::with_capacity(64)),
        }
    }

    fn with<F>(&self, f: impl FnOnce() -> F) -> F {
        struct Entered<'a> {
            current: &'a Cell<Option<NonNull<Scheduler>>>,
        }

        impl<'a> Drop for Entered<'a> {
            fn drop(&mut self) {
                self.current.set(None);
            }
        }

        CURRENT_TASK_SET.with(|current| {
            current.set(Some(NonNull::from(self)));
            let _entered = Entered { current };
            f()
        })
    }

    fn is_current(&self) -> bool {
        CURRENT_TASK_SET
            .try_with(|current| {
                current
                    .get()
                    .iter()
                    .any(|current| ptr::eq(current.as_ptr(), self as *const _))
            })
            .unwrap_or(false)
    }

    fn next_task(&self) -> Option<UnsendTask<Self>> {
        unsafe { (*self.queue.get()).pop_front() }
    }

    fn tick(&self) {
        assert!(self.is_current());
        for _ in 0..MAX_TASKS_PER_TICK {
            let task = match self.next_task() {
                Some(task) => task,
                None => return,
            };

            if let Some(task) = task.run(&mut || Some(self.into())) {
                self.schedule(task);
            }
        }
    }
}

unsafe impl Send for Scheduler {}
unsafe impl Sync for Scheduler {}

impl fmt::Debug for Scheduler {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Scheduler { .. }").finish()
    }
}
impl Drop for Scheduler {
    fn drop(&mut self) {
        // Drain all local tasks
        while let Some(task) = self.next_task() {
            task.shutdown();
        }

        // Release owned tasks
        unsafe {
            (*self.tasks.get()).shutdown();
        }
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;
    use crate::runtime;

    #[test]
    fn local_current_thread() {
        let mut rt = runtime::Builder::new().current_thread().build().unwrap();
        TaskGroup::new().block_on(&mut rt, async {
            spawn(async {}).await.unwrap();
        });
    }

    #[test]
    fn local_threadpool() {
        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Runtime::new().unwrap();
        TaskGroup::new().block_on(&mut rt, async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            spawn(async {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
            })
            .await
            .unwrap();
        });
    }

    #[test]
    fn local_threadpool_timer() {
        // This test ensures that runtime services like the timer are properly
        // set for the local task group.
        use std::time::Duration;
        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Runtime::new().unwrap();
        TaskGroup::new().block_on(&mut rt, async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            let join = spawn(async move {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
                crate::time::delay_for(Duration::from_millis(10)).await;
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
            });
            join.await.unwrap();
        });
    }

    #[test]
    // This will panic, since the thread that calls `block_on` cannot use
    // in-place blocking inside of `block_on`.
    #[should_panic]
    fn local_threadpool_blocking_in_place() {
        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Runtime::new().unwrap();
        TaskGroup::new().block_on(&mut rt, async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            let join = spawn(async move {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
                runtime::blocking::in_place(|| {});
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
            });
            join.await.unwrap();
        });
    }

    #[test]
    fn local_threadpool_blocking_run() {
        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Runtime::new().unwrap();
        TaskGroup::new().block_on(&mut rt, async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            let join = spawn(async move {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
                runtime::blocking::run(|| {
                    assert!(
                        !ON_RT_THREAD.with(|cell| cell.get()),
                        "blocking must not run on the local task group's thread"
                    );
                })
                .await;
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
            });
            join.await.unwrap();
        });
    }

    #[test]
    fn all_spawns_are_local() {
        use futures_util::future;

        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Runtime::new().unwrap();
        TaskGroup::new().block_on(&mut rt, async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            let handles = (0..128)
                .map(|_| {
                    spawn(async {
                        assert!(ON_RT_THREAD.with(|cell| cell.get()));
                    })
                })
                .collect::<Vec<_>>();
            for result in future::join_all(handles).await {
                result.unwrap();
            }
        })
    }

    #[test]
    fn nested_spawn_is_local() {
        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Runtime::new().unwrap();
        TaskGroup::new().block_on(&mut rt, async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            spawn(async {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
                spawn(async {
                    assert!(ON_RT_THREAD.with(|cell| cell.get()));
                    spawn(async {
                        assert!(ON_RT_THREAD.with(|cell| cell.get()));
                        spawn(async {
                            assert!(ON_RT_THREAD.with(|cell| cell.get()));
                        })
                        .await
                        .unwrap();
                    })
                    .await
                    .unwrap();
                })
                .await
                .unwrap();
            })
            .await
            .unwrap();
        })
    }
}
