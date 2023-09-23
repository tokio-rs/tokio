//! Types related to the [`TaskTracker`] collection.
//!
//! See the documentation of [`TaskTracker`] for more information.

use pin_project_lite::pin_project;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::runtime::Handle;
use tokio::sync::{futures::Notified, Notify};
use tokio::task::{JoinHandle, LocalSet};

/// A task tracker used for waiting until tasks exit.
///
/// This is usually used together with [`CancellationToken`] to implement [graceful shutdown]. The
/// `CancellationToken` is used to signal to tasks that they should shut down, and the
/// `TaskTracker` is used to wait for them to finish shutting down.
///
/// The `TaskTracker` will also keep track of a `closed` boolean. This is used to handle the case
/// where the `TaskTracker` is empty, but we don't want to shut down yet. This means that the
/// [`wait`] method will wait until *both* of the following happen at the same time:
///
///  * The `TaskTracker` must be closed using the [`close`] method.
///  * The `TaskTracker` must be empty, that is, all tasks that it is tracking must have exited.
///
/// # Comparison to `JoinSet`
///
/// The main Tokio crate has a similar collection known as [`JoinSet`]. The `JoinSet` type has a
/// lot more features than `TaskTracker`, so you should only use a `TaskTracker` when you need one
/// of its unique features:
///
///  1. When tasks exit, a `TaskTracker` will allow the task to immediately free its memory.
///  2. By not closing the `TaskTracker`, you can prevent [`wait`] from returning even if the
///     `TaskTracker` is empty.
///  3. A `TaskTracker` does not require mutable access to insert tasks.
///
/// The first point is the most important one. With a [`JoinSet`], it keeps track of the return
/// value of every task you insert into it. This means that if you keep inserting tasks and never
/// call [`join_next`], then their return values will keep building up and take up memory, even
/// if most of the tasks have already exited. This can cause you to run out of memory. With a
/// `TaskTracker`, this does not happen. Once tasks exit, they are immediately removed from the
/// `TaskTracker`.
///
/// # Examples
///
/// ## Spawn tasks and wait for them to exit
///
/// This is a simple example. For this case, you should probably use a [`JoinSet`] instead.
///
/// ```
/// use tokio_util::sync::TaskTracker;
///
/// #[tokio::main]
/// async fn main() {
///     let tracker = TaskTracker::new();
///
///     for i in 0..10 {
///         tracker.spawn(async move {
///             println!("Task {} is running!", i);
///         });
///     }
///     // Once we spawned everything, we close the tracker.
///     tracker.close();
///
///     // Wait for everything to finish.
///     tracker.wait().await;
///
///     println!("This is printed after all of the tasks.");
/// }
/// ```
///
/// ## Wait for tasks to exit
///
/// This example shows the intended use-case of `TaskTracker`. It is used together with
/// [`CancellationToken`] to implement graceful shutdown.
/// ```
/// use tokio_util::sync::{CancellationToken, TaskTracker};
/// use tokio::time::{self, Duration};
///
/// async fn background_task(num: u64) {
///     for i in 0..10 {
///         time::sleep(Duration::from_millis(100*num)).await;
///         println!("Background task {} in iteration {}.", num, i);
///     }
/// }
///
/// #[tokio::main]
/// # async fn _hidden() {}
/// # #[tokio::main(flavor = "current_thread", start_paused = true)]
/// async fn main() {
///     let tracker = TaskTracker::new();
///     let token = CancellationToken::new();
///
///     for i in 0..10 {
///         let token = token.clone();
///         tracker.spawn(async move {
///             // Use a `tokio::select!` to kill the background task if the token is
///             // cancelled.
///             tokio::select! {
///                 () = background_task(i) => {
///                     println!("Task {} exiting normally.", i);
///                 },
///                 () = token.cancelled() => {
///                     // Do some cleanup before we really exit.
///                     time::sleep(Duration::from_millis(50)).await;
///                     println!("Task {} finished cleanup.", i);
///                 },
///             }
///         });
///     }
///
///     // Spawn a background task that will send the shutdown signal.
///     {
///         let tracker = tracker.clone();
///         tokio::spawn(async move {
///             // Normally you would use something like ctrl-c instead of
///             // sleeping.
///             time::sleep(Duration::from_secs(2)).await;
///             tracker.close();
///             token.cancel();
///         });
///     }
///
///     // Wait for all tasks to exit.
///     tracker.wait().await;
///
///     println!("All tasks have exited now.");
/// }
/// ```
///
/// [`wait`]: Self::wait
/// [`close`]: Self::close
/// [`CancellationToken`]: crate::sync::CancellationToken
/// [`JoinSet`]: tokio::task::JoinSet
/// [`join_next`]: tokio::task::JoinSet::join_next
/// [graceful shutdown]: https://tokio.rs/tokio/topics/shutdown
#[derive(Clone)]
#[repr(transparent)]
pub struct TaskTracker {
    inner: Arc<TaskTrackerInner>,
}

/// Represents a task tracked by a [`TaskTracker`].
#[must_use]
#[derive(Debug)]
pub struct TaskTrackerToken {
    task_tracker: TaskTracker,
}

struct TaskTrackerInner {
    /// Keeps track of the state.
    ///
    /// The lowest bit is whether the task tracker is closed.
    ///
    /// The rest of the bits count the number of tracked tasks.
    state: AtomicUsize,
    /// Used to notify when the last task exits.
    on_last_exit: Notify,
}

pin_project! {
    /// A future that is tracked as a task by a [`TaskTracker`].
    ///
    /// The associated [`TaskTracker`] cannot complete until this future is dropped.
    ///
    /// This future is returned by [`TaskTracker::track_future`].
    #[must_use = "futures do nothing unless polled"]
    pub struct TrackedFuture<F> {
        #[pin]
        future: F,
        token: TaskTrackerToken,
    }
}

pin_project! {
    /// A future that completes when the [`TaskTracker`] is empty and closed.
    ///
    /// This future is returned by [`TaskTracker::wait`].
    #[must_use = "futures do nothing unless polled"]
    pub struct TaskTrackerWaitFuture<'a> {
        #[pin]
        future: Notified<'a>,
        inner: &'a TaskTrackerInner,
    }
}

impl TaskTrackerInner {
    #[inline]
    fn new() -> Self {
        Self {
            state: AtomicUsize::new(0),
            on_last_exit: Notify::new(),
        }
    }

    #[inline]
    fn is_done(&self) -> bool {
        // If empty and closed bit set, then we are done.
        //
        // The acquire load will synchronize with the release store of any previous call to
        // `set_closed` and `drop_task`.
        self.state.load(Ordering::Acquire) == 1
    }

    #[inline]
    fn set_closed(&self) -> bool {
        // The AcqRel ordering makes the closed bit behave like a `Mutex<bool>` for synchronization
        // purposes. We do this because it makes the return value of `TaskTracker::{close,reopen}`
        // more meaningful for the user. Without these orderings, this assert could fail:
        // ```
        // // thread 1
        // some_other_atomic.store(true, Relaxed);
        // tracker.close();
        //
        // // thread 2
        // if tracker.reopen() {
        //     assert!(some_other_atomic.load(Relaxed));
        // }
        // ```
        // However, with the AcqRel ordering, we establish a happens-before relationship from the
        // call to `close` and the later call to `reopen` that returned true.
        let state = self.state.fetch_or(1, Ordering::AcqRel);

        // If there are no tasks, and if it was not already closed:
        if state == 0 {
            self.notify_now();
        }

        (state & 1) == 0
    }

    #[inline]
    fn set_open(&self) -> bool {
        // See `set_closed` regarding the AcqRel ordering.
        let state = self.state.fetch_and(!1, Ordering::AcqRel);
        (state & 1) == 1
    }

    #[inline]
    fn add_task(&self) {
        self.state.fetch_add(2, Ordering::Relaxed);
    }

    #[inline]
    fn drop_task(&self) {
        let state = self.state.fetch_sub(2, Ordering::Release);

        // If this was the last task and we are closed:
        if state == 3 {
            self.notify_now();
        }
    }

    #[cold]
    fn notify_now(&self) {
        // Insert an acquire fence. This matters for `drop_task` but doesn't matter for
        // `set_closed` since it already uses AcqRel.
        //
        // This synchronizes with the release store of any other call to `drop_task`, and with the
        // release store in the call to `set_closed`. That ensures that everything that happened
        // before those other calls to `drop_task` or `set_closed` will be visible after this load,
        // and those things will also be visible to anything woken by the call to `notify_waiters`.
        self.state.load(Ordering::Acquire);

        self.on_last_exit.notify_waiters();
    }
}

impl TaskTracker {
    /// Create a new `TaskTracker`.
    ///
    /// The `TaskTracker` will start out as open.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(TaskTrackerInner::new()),
        }
    }

    /// Waits until this `TaskTracker` is both closed and empty.
    ///
    /// The `wait` future will resolve if the `TaskTracker` has been both closed and empty at the
    /// same time at any point since the `wait` future was created. This is the case even if more
    /// tasks have been spawned by the time that the `wait` future is polled.
    #[inline]
    pub fn wait(&self) -> TaskTrackerWaitFuture<'_> {
        TaskTrackerWaitFuture {
            future: self.inner.on_last_exit.notified(),
            inner: &self.inner,
        }
    }

    /// Close this `TaskTracker`.
    ///
    /// This allows `wait` futures to complete. It does not prevent you from spawning new tasks.
    ///
    /// Returns `true` if this closed the `TaskTracker`, or `false` if it was already closed.
    #[inline]
    pub fn close(&self) -> bool {
        self.inner.set_closed()
    }

    /// Reopen this `TaskTracker`.
    ///
    /// This prevents `wait` futures from completing even if the `TaskTracker` is empty.
    ///
    /// Returns `true` if this reopened the `TaskTracker`, or `false` if it was already open.
    #[inline]
    pub fn reopen(&self) -> bool {
        self.inner.set_open()
    }

    /// Returns true if this `TaskTracker` is closed.
    #[inline]
    pub fn is_closed(&self) -> bool {
        (self.inner.state.load(Ordering::Acquire) & 1) != 0
    }

    /// Returns true if this `TaskTracker` is open.
    #[inline]
    pub fn is_open(&self) -> bool {
        !self.is_closed()
    }

    /// Returns the number of tasks tracked by this `TaskTracker`.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.state.load(Ordering::Acquire) >> 1
    }

    /// Returns true if there are no tasks in this `TaskTracker`.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.state.load(Ordering::Acquire) <= 1
    }

    /// Spawn the provided future on the Tokio runtime, and track it in this `TaskTracker`.
    #[inline]
    #[track_caller]
    #[cfg(feature = "rt")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
    pub fn spawn<F>(&self, task: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        tokio::task::spawn(self.track_future(task))
    }

    /// Spawn the provided future on the Tokio runtime, and track it in this `TaskTracker`.
    #[inline]
    #[track_caller]
    #[cfg(feature = "rt")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
    pub fn spawn_on<F>(&self, task: F, handle: &Handle) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        handle.spawn(self.track_future(task))
    }

    /// Spawn the provided future on the current [`LocalSet`], and track it in this `TaskTracker`.
    ///
    /// [`LocalSet`]: tokio::task::LocalSet
    #[inline]
    #[track_caller]
    #[cfg(feature = "rt")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
    pub fn spawn_local<F>(&self, task: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        tokio::task::spawn_local(self.track_future(task))
    }

    /// Spawn the provided future on the provided [`LocalSet`], and track it in this `TaskTracker`.
    ///
    /// [`LocalSet`]: tokio::task::LocalSet
    #[inline]
    #[track_caller]
    #[cfg(feature = "rt")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
    pub fn spawn_local_on<F>(&self, task: F, local_set: &LocalSet) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        local_set.spawn_local(self.track_future(task))
    }

    /// Spawn the provided blocking task on the current Tokio runtime, and track it in this `TaskTracker`.
    #[inline]
    #[track_caller]
    #[cfg(feature = "rt")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
    pub fn spawn_blocking<F, T>(&self, task: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T,
        F: Send + 'static,
        T: Send + 'static,
    {
        let token = self.token();
        tokio::task::spawn_blocking(move || {
            let res = task();
            drop(token);
            res
        })
    }

    /// Spawn the provided blocking task on the provided Tokio runtime, and track it in this `TaskTracker`.
    #[inline]
    #[track_caller]
    #[cfg(feature = "rt")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
    pub fn spawn_blocking_on<F, T>(&self, task: F, handle: &Handle) -> JoinHandle<T>
    where
        F: FnOnce() -> T,
        F: Send + 'static,
        T: Send + 'static,
    {
        let token = self.token();
        handle.spawn_blocking(move || {
            let res = task();
            drop(token);
            res
        })
    }

    /// Track the provided future.
    ///
    /// The returned [`TrackedFuture`] will count as a task tracked by this collection, and will
    /// prevent calls to [`wait`] from returning.
    ///
    /// The task is removed from the collection when it is dropped. (It is not removed immediately
    /// when [`poll`] returns [`Poll::Pending`]. You have to actually run the destructor for it to
    /// be removed.)
    ///
    /// [`wait`]: Self::wait
    /// [`poll`]: std::future::Future::poll
    /// [`Poll::Pending`]: std::task::Poll::Pending
    #[inline]
    pub fn track_future<F: Future>(&self, future: F) -> TrackedFuture<F> {
        TrackedFuture {
            future,
            token: self.token(),
        }
    }

    /// Creates a [`TaskTrackerToken`] representing a task tracked by this `TaskTracker`.
    ///
    /// This token is a lower-level utility than the spawn methods. Each token is considered to
    /// correspond to a task. As long as the token exists, the `TaskTracker` cannot complete.
    /// Furthermore, the count returned by the [`len`] method will include the tokens in the count.
    ///
    /// Dropping the token corresponds to when the task exits.
    ///
    /// [`len`]: TaskTracker::len
    #[inline]
    pub fn token(&self) -> TaskTrackerToken {
        self.inner.add_task();
        TaskTrackerToken {
            task_tracker: self.clone(),
        }
    }
}

impl Default for TaskTracker {
    #[inline]
    fn default() -> TaskTracker {
        TaskTracker::new()
    }
}

fn debug_inner(inner: &TaskTrackerInner, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let state = inner.state.load(Ordering::Acquire);
    let is_closed = (state & 1) != 0;
    let len = state >> 1;

    f.debug_struct("TaskTracker")
        .field("len", &len)
        .field("is_closed", &is_closed)
        .field("inner", &(inner as *const TaskTrackerInner))
        .finish()
}

impl fmt::Debug for TaskTracker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_inner(&self.inner, f)
    }
}

impl TaskTrackerToken {
    /// Returns the [`TaskTracker`] that this token is associated with.
    #[inline]
    pub fn task_tracker(&self) -> &TaskTracker {
        &self.task_tracker
    }
}

impl Clone for TaskTrackerToken {
    #[inline]
    fn clone(&self) -> TaskTrackerToken {
        self.task_tracker.inner.add_task();
        TaskTrackerToken {
            task_tracker: self.task_tracker.clone(),
        }
    }
}

impl Drop for TaskTrackerToken {
    #[inline]
    fn drop(&mut self) {
        self.task_tracker.inner.drop_task();
    }
}

impl<F: Future> Future for TrackedFuture<F> {
    type Output = F::Output;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        self.project().future.poll(cx)
    }
}

impl<F: fmt::Debug> fmt::Debug for TrackedFuture<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrackedFuture")
            .field("future", &self.future)
            .field("task_tracker", self.token.task_tracker())
            .finish()
    }
}

impl<'a> Future for TaskTrackerWaitFuture<'a> {
    type Output = ();

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let me = self.project();

        if me.inner.is_done() {
            return Poll::Ready(());
        }

        me.future.poll(cx)
    }
}

impl<'a> fmt::Debug for TaskTrackerWaitFuture<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct Helper<'a>(&'a TaskTrackerInner);

        impl fmt::Debug for Helper<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                debug_inner(self.0, f)
            }
        }

        f.debug_struct("TaskTrackerWaitFuture")
            .field("future", &self.future)
            .field("task_tracker", &Helper(self.inner))
            .finish()
    }
}
