use crate::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::task::TaskSet;

/// A wrapper around [`TaskSet`] that implements [`Stream`].
/// It automatically propagates panics. You should manully poll
/// a task set if you want to handle them.
///
/// [`TaskSet`]: struct@tokio::task::TaskSet
/// [`Stream`]: trait@crate::Stream
#[derive(Debug)]
#[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
pub struct TaskSetStream<T> {
    inner: TaskSet<T>,
}

impl<T> TaskSetStream<T> {
    /// Create a new `TaskSetStream`.
    pub fn new(task_set: TaskSet<T>) -> Self {
        Self { inner: task_set }
    }

    /// Get back the inner `TaskSet`.
    pub fn into_inner(self) -> TaskSet<T> {
        self.inner
    }
}

impl<T> Stream for TaskSetStream<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        Pin::into_inner(self).inner.poll_next_finished(cx)
    }
}

impl<T> AsRef<TaskSet<T>> for TaskSetStream<T> {
    fn as_ref(&self) -> &TaskSet<T> {
        &self.inner
    }
}

impl<T> AsMut<TaskSet<T>> for TaskSetStream<T> {
    fn as_mut(&mut self) -> &mut TaskSet<T> {
        &mut self.inner
    }
}
