//! Futures task based helpers to easily test futures and manually written futures.
//!
//! The [`Spawn`] type is used as a mock task harness that allows you to poll futures
//! without needing to setup pinning or context. Any future can be polled but if the
//! future requires the tokio async context you will need to ensure that you poll the
//! [`Spawn`] within a tokio context, this means that as long as you are inside the
//! runtime it will work and you can poll it via [`Spawn`].
//!
//! [`Spawn`] also supports [`Stream`] to call `poll_next` without pinning
//! or context.
//!
//! In addition to circumventing the need for pinning and context, [`Spawn`] also tracks
//! the amount of times the future/task was woken. This can be useful to track if some
//! leaf future notified the root task correctly.
//!
//! # Example
//!
//! ```
//! use tokio_test::task;
//!
//! let fut = async {};
//!
//! let mut task = task::spawn(fut);
//!
//! assert!(task.poll().is_ready(), "Task was not ready!");
//! ```

use std::future::Future;
use std::ops;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use tokio_stream::Stream;

/// Spawn a future into a [`Spawn`] which wraps the future in a mocked executor.
///
/// This can be used to spawn a [`Future`] or a [`Stream`].
///
/// For more information, check the module docs.
pub fn spawn<T>(task: T) -> Spawn<T> {
    Spawn {
        task: MockTask::new(),
        future: Box::pin(task),
    }
}

/// Future spawned on a mock task that can be used to poll the future or stream
/// without needing pinning or context types.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Spawn<T> {
    task: MockTask,
    future: Pin<Box<T>>,
}

#[derive(Debug, Clone, Default)]
struct MockTask {
    waker_state: Arc<WakerState>,
}

#[derive(Debug, Default)]
struct WakerState {
    is_woken: AtomicBool,
}

impl<T> Spawn<T> {
    /// Consumes `self` returning the inner value
    pub fn into_inner(self) -> T
    where
        T: Unpin,
    {
        *Pin::into_inner(self.future)
    }

    /// Returns `true` if the inner future has received a wake notification
    /// since the last call to `enter`.
    pub fn is_woken(&self) -> bool {
        self.task.is_woken()
    }

    /// Returns the number of references to the task waker
    ///
    /// The task itself holds a reference. The return value will never be zero.
    pub fn waker_ref_count(&self) -> usize {
        self.task.waker_ref_count()
    }

    /// Enter the task context
    pub fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Context<'_>, Pin<&mut T>) -> R,
    {
        let fut = self.future.as_mut();
        self.task.enter(|cx| f(cx, fut))
    }
}

impl<T: Unpin> ops::Deref for Spawn<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.future
    }
}

impl<T: Unpin> ops::DerefMut for Spawn<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.future
    }
}

impl<T: Future> Spawn<T> {
    /// If `T` is a [`Future`] then poll it. This will handle pinning and the context
    /// type for the future.
    pub fn poll(&mut self) -> Poll<T::Output> {
        let fut = self.future.as_mut();
        self.task.enter(|cx| fut.poll(cx))
    }
}

impl<T: Stream> Spawn<T> {
    /// If `T` is a [`Stream`] then `poll_next` it. This will handle pinning and the context
    /// type for the stream.
    pub fn poll_next(&mut self) -> Poll<Option<T::Item>> {
        let stream = self.future.as_mut();
        self.task.enter(|cx| stream.poll_next(cx))
    }
}

impl<T: Future> Future for Spawn<T> {
    type Output = T::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.future.as_mut().poll(cx)
    }
}

impl<T: Stream> Stream for Spawn<T> {
    type Item = T::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.future.as_mut().poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.future.size_hint()
    }
}

impl MockTask {
    /// Creates new mock task
    fn new() -> Self {
        MockTask::default()
    }

    /// Runs a closure from the context of the task.
    ///
    /// Any wake notifications resulting from the execution of the closure are
    /// tracked.
    fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Context<'_>) -> R,
    {
        self.waker_state.is_woken.store(false, Ordering::Relaxed);
        let waker = Waker::from(self.waker_state.clone());
        let mut cx = Context::from_waker(&waker);
        f(&mut cx)
    }

    /// Returns `true` if the inner future has received a wake notification
    /// since the last call to `enter`.
    fn is_woken(&self) -> bool {
        self.waker_state.is_woken.load(Ordering::Relaxed)
    }

    /// Returns the number of references to the task waker
    ///
    /// The task itself holds a reference. The return value will never be zero.
    fn waker_ref_count(&self) -> usize {
        Arc::strong_count(&self.waker_state)
    }
}

impl Wake for WakerState {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.is_woken.store(true, Ordering::Release);
    }
}
