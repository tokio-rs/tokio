//! Runs `!Send` futures on the current thread.
use crate::sync::AtomicWaker;
use crate::task::{self, queue::MpscQueues, JoinHandle, Schedule, Task};

use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

cfg_rt_util! {
    /// A set of tasks which are executed on the same thread.
    ///
    /// In some cases, it is necessary to run one or more futures that do not
    /// implement [`Send`] and thus are unsafe to send between threads. In these
    /// cases, a [local task set] may be used to schedule one or more `!Send`
    /// futures to run together on the same thread.
    ///
    /// For example, the following code will not compile:
    ///
    /// ```rust,compile_fail
    /// use std::rc::Rc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     // `Rc` does not implement `Send`, and thus may not be sent between
    ///     // threads safely.
    ///     let unsend_data = Rc::new("my unsend data...");
    ///
    ///     let unsend_data = unsend_data.clone();
    ///     // Because the `async` block here moves `unsend_data`, the future is `!Send`.
    ///     // Since `tokio::spawn` requires the spawned future to implement `Send`, this
    ///     // will not compile.
    ///     tokio::spawn(async move {
    ///         println!("{}", unsend_data);
    ///         // ...
    ///     }).await.unwrap();
    /// }
    /// ```
    /// In order to spawn `!Send` futures, we can use a local task set to
    /// schedule them on the thread calling [`Runtime::block_on`]. When running
    /// inside of the local task set, we can use [`task::spawn_local`], which can
    /// spawn `!Send` futures. For example:
    ///
    /// ```rust
    /// use std::rc::Rc;
    /// use tokio::task;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let unsend_data = Rc::new("my unsend data...");
    ///
    ///     // Construct a local task set that can run `!Send` futures.
    ///     let local = task::LocalSet::new();
    ///
    ///     // Run the local task set.
    ///     local.run_until(async move {
    ///         let unsend_data = unsend_data.clone();
    ///         // `spawn_local` ensures that the future is spawned on the local
    ///         // task set.
    ///         task::spawn_local(async move {
    ///             println!("{}", unsend_data);
    ///             // ...
    ///         }).await.unwrap();
    ///     }).await;
    /// }
    /// ```
    ///
    /// ## Awaiting a `LocalSet`
    ///
    /// Additionally, a `LocalSet` itself implements `Future`, completing when
    /// *all* tasks spawned on the `LocalSet` complete. This can be used to run
    /// several futures on a `LocalSet` and drive the whole set until they
    /// complete. For example,
    ///
    /// ```rust
    /// use tokio::{task, time};
    /// use std::rc::Rc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let unsend_data = Rc::new("world");
    ///     let local = task::LocalSet::new();
    ///
    ///     let unsend_data2 = unsend_data.clone();
    ///     local.spawn_local(async move {
    ///         // ...
    ///         println!("hello {}", unsend_data2)
    ///     });
    ///
    ///     local.spawn_local(async move {
    ///         time::delay_for(time::Duration::from_millis(100)).await;
    ///         println!("goodbye {}", unsend_data)
    ///     });
    ///
    ///     // ...
    ///
    ///     local.await;
    /// }
    /// ```
    ///
    /// [`Send`]: https://doc.rust-lang.org/std/marker/trait.Send.html
    /// [local task set]: struct.LocalSet.html
    /// [`Runtime::block_on`]: ../struct.Runtime.html#method.block_on
    /// [`task::spawn_local`]: fn.spawn.html
    #[derive(Debug)]
    pub struct LocalSet {
        scheduler: Rc<Scheduler>,
    }
}

#[derive(Debug)]
struct Scheduler {
    tick: Cell<u8>,

    queues: MpscQueues<Self>,

    /// Used to notify the `LocalFuture` when a task in the local task set is
    /// notified.
    waker: AtomicWaker,
}

pin_project! {
    #[derive(Debug)]
    struct LocalFuture<F> {
        scheduler: Rc<Scheduler>,
        #[pin]
        future: F,
    }
}

thread_local! {
    static CURRENT_TASK_SET: Cell<Option<NonNull<Scheduler>>> = Cell::new(None);
}

cfg_rt_util! {
    /// Spawns a `!Send` future on the local task set.
    ///
    /// The spawned future will be run on the same thread that called `spawn_local.`
    /// This may only be called from the context of a local task set.
    ///
    /// # Panics
    ///
    /// - This function panics if called outside of a local task set.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::rc::Rc;
    /// use tokio::task;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let unsend_data = Rc::new("my unsend data...");
    ///
    ///     let local = task::LocalSet::new();
    ///
    ///     // Run the local task set.
    ///     local.run_until(async move {
    ///         let unsend_data = unsend_data.clone();
    ///         task::spawn_local(async move {
    ///             println!("{}", unsend_data);
    ///             // ...
    ///         }).await.unwrap();
    ///     }).await;
    /// }
    /// ```
    pub fn spawn_local<F>(future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        CURRENT_TASK_SET.with(|current| {
            let current = current
                .get()
                .expect("`spawn_local` called from outside of a task::LocalSet!");
            let (task, handle) = task::joinable_local(future);
            unsafe {
                // safety: this function is unsafe to call outside of the local
                // thread. Since the call above to get the current task set
                // would not succeed if we were outside of a local set, this is
                // safe.
                current.as_ref().queues.push_local(task);
            }

            handle
        })
    }
}

/// Max number of tasks to poll per tick.
const MAX_TASKS_PER_TICK: usize = 61;

impl LocalSet {
    /// Returns a new local task set.
    pub fn new() -> Self {
        Self {
            scheduler: Rc::new(Scheduler::new()),
        }
    }

    /// Spawns a `!Send` task onto the local task set.
    ///
    /// This task is guaranteed to be run on the current thread.
    ///
    /// Unlike the free function [`spawn_local`], this method may be used to
    /// spawn local tasks when the task set is _not_ running. For example:
    /// ```rust
    /// use tokio::task;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let local = task::LocalSet::new();
    ///
    ///     // Spawn a future on the local set. This future will be run when
    ///     // we call `run_until` to drive the task set.
    ///     local.spawn_local(async {
    ///        // ...
    ///     });
    ///
    ///     // Run the local task set.
    ///     local.run_until(async move {
    ///         // ...
    ///     }).await;
    ///
    ///     // When `run` finishes, we can spawn _more_ futures, which will
    ///     // run in subsequent calls to `run_until`.
    ///     local.spawn_local(async {
    ///        // ...
    ///     });
    ///
    ///     local.run_until(async move {
    ///         // ...
    ///     }).await;
    /// }
    /// ```
    /// [`spawn_local`]: fn.spawn_local.html
    pub fn spawn_local<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (task, handle) = task::joinable_local(future);
        unsafe {
            // safety: since `LocalSet` is not Send or Sync, this is
            // always being called from the local thread.
            self.scheduler.queues.push_local(task);
        }
        handle
    }

    /// Run a future to completion on the provided runtime, driving any local
    /// futures spawned on this task set on the current thread.
    ///
    /// This runs the given future on the runtime, blocking until it is
    /// complete, and yielding its resolved result. Any tasks or timers which
    /// the future spawns internally will be executed on the runtime. The future
    /// may also call [`spawn_local`] to spawn_local additional local futures on the
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
    /// futures in the local task set inside that call to `block_on`, the local
    /// futures may not use [in-place blocking]. If a blocking call needs to be
    /// issued from a local task, the [`spawn_blocking`] API may be used instead.
    ///
    /// For example, this will panic:
    /// ```should_panic
    /// use tokio::runtime::Runtime;
    /// use tokio::task;
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// let local = task::LocalSet::new();
    /// local.block_on(&mut rt, async {
    ///     let join = task::spawn_local(async {
    ///         let blocking_result = task::block_in_place(|| {
    ///             // ...
    ///         });
    ///         // ...
    ///     });
    ///     join.await.unwrap();
    /// })
    /// ```
    /// This, however, will not panic:
    /// ```
    /// use tokio::runtime::Runtime;
    /// use tokio::task;
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// let local = task::LocalSet::new();
    /// local.block_on(&mut rt, async {
    ///     let join = task::spawn_local(async {
    ///         let blocking_result = task::spawn_blocking(|| {
    ///             // ...
    ///         }).await;
    ///         // ...
    ///     });
    ///     join.await.unwrap();
    /// })
    /// ```
    ///
    /// [`spawn_local`]: fn.spawn_local.html
    /// [`Runtime::block_on`]: ../struct.Runtime.html#method.block_on
    /// [in-place blocking]: ../blocking/fn.in_place.html
    /// [`spawn_blocking`]: ../blocking/fn.spawn_blocking.html
    pub fn block_on<F>(&self, rt: &mut crate::runtime::Runtime, future: F) -> F::Output
    where
        F: Future,
    {
        rt.block_on(self.run_until(future))
    }

    /// Run a future to completion on the local set, returning its output.
    ///
    /// This returns a future that runs the given future with a local set,
    /// allowing it to call [`spawn_local`] to spawn additional `!Send` futures.
    /// Any local futures spawned on the local set will be driven in the
    /// background until the future passed to `run_until` completes. When the future
    /// passed to `run` finishes, any local futures which have not completed
    /// will remain on the local set, and will be driven on subsequent calls to
    /// `run_until` or when [awaiting the local set] itself.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tokio::task;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     task::LocalSet::new().run_until(async {
    ///         task::spawn_local(async move {
    ///             // ...
    ///         }).await.unwrap();
    ///         // ...
    ///     }).await;
    /// }
    /// ```
    ///
    /// [`spawn_local`]: fn.spawn_local.html
    /// [awaiting the local set]: #awaiting-a-localset
    pub async fn run_until<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        let scheduler = self.scheduler.clone();
        let future = LocalFuture { scheduler, future };
        future.await
    }
}

impl Future for LocalSet {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let scheduler = self.as_ref().scheduler.clone();
        scheduler.waker.register_by_ref(cx.waker());

        if scheduler.with(|| scheduler.tick()) {
            // If `tick` returns true, we need to notify the local future again:
            // there are still tasks remaining in the run queue.
            cx.waker().wake_by_ref();
            Poll::Pending
        } else if scheduler.is_empty() {
            // If the scheduler has no remaining futures, we're done!
            Poll::Ready(())
        } else {
            // There are still futures in the local set, but we've polled all the
            // futures in the run queue. Therefore, we can just return Pending
            // since the remaining futures will be woken from somewhere else.
            Poll::Pending
        }
    }
}

impl Default for LocalSet {
    fn default() -> Self {
        Self::new()
    }
}

// === impl LocalFuture ===

impl<F: Future> Future for LocalFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let scheduler = this.scheduler;
        let mut future = this.future;
        scheduler.waker.register_by_ref(cx.waker());
        scheduler.with(|| {
            if let Poll::Ready(output) = future.as_mut().poll(cx) {
                return Poll::Ready(output);
            }

            if scheduler.tick() {
                // If `tick` returns true, we need to notify the local future again:
                // there are still tasks remaining in the run queue.
                cx.waker().wake_by_ref();
            }

            Poll::Pending
        })
    }
}

// === impl Scheduler ===

impl Schedule for Scheduler {
    fn bind(&self, task: &Task<Self>) {
        assert!(self.is_current());
        unsafe {
            self.queues.add_task(task);
        }
    }

    fn release(&self, task: Task<Self>) {
        // This will be called when dropping the local runtime.
        self.queues.release_remote(task);
    }

    fn release_local(&self, task: &Task<Self>) {
        debug_assert!(self.is_current());
        unsafe {
            self.queues.release_local(task);
        }
    }

    fn schedule(&self, task: Task<Self>) {
        if self.is_current() {
            unsafe { self.queues.push_local(task) };
        } else {
            let mut lock = self.queues.remote();
            lock.schedule(task, false);

            self.waker.wake();

            drop(lock);
        }
    }
}

impl Scheduler {
    fn new() -> Self {
        Self {
            tick: Cell::new(0),
            queues: MpscQueues::new(),
            waker: AtomicWaker::new(),
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
            let prev = current.replace(Some(NonNull::from(self)));
            assert!(prev.is_none(), "nested call to local::Scheduler::with");
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

    /// Tick the scheduler, returning whether the local future needs to be
    /// notified again.
    fn tick(&self) -> bool {
        assert!(self.is_current());
        for _ in 0..MAX_TASKS_PER_TICK {
            let tick = self.tick.get().wrapping_add(1);
            self.tick.set(tick);

            let task = match unsafe {
                // safety: we must be on the local thread to call this. The assertion
                // the top of this method ensures that `tick` is only called locally.
                self.queues.next_task(tick)
            } {
                Some(task) => task,
                // We have fully drained the queue of notified tasks, so the
                // local future doesn't need to be notified again — it can wait
                // until something else wakes a task in the local set.
                None => return false,
            };

            if let Some(task) = task.run(&mut || Some(self.into())) {
                unsafe {
                    // safety: we must be on the local thread to call this. The
                    // the top of this method ensures that `tick` is only called locally.
                    self.queues.push_local(task);
                }
            }
        }

        true
    }

    fn is_empty(&self) -> bool {
        unsafe {
            // safety: this method may not be called from threads other than the
            // thread that owns the `Queues`. since `Scheduler` is not `Send` or
            // `Sync`, that shouldn't happen.
            !self.queues.has_tasks_remaining()
        }
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        unsafe {
            // safety: these functions are unsafe to call outside of the local
            // thread. Since the `Scheduler` type is not `Send` or `Sync`, we
            // know it will be dropped only from the local thread.
            self.queues.shutdown();

            // Wait until all tasks have been released.
            // XXX: this is a busy loop, but we don't really have any way to park
            // the thread here?
            loop {
                self.queues.drain_pending_drop();
                self.queues.drain_queues();

                if !self.queues.has_tasks_remaining() {
                    break;
                }

                std::thread::yield_now();
            }
        }
    }
}
