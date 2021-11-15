use crate::Stream;
use std::task::Context;
use tokio::macros::support::{Pin, Poll};
use tokio::task::JoinError;
use tokio_util::taskset::TaskSet;

/// A wrapper around [`TaskSet`] that implements [`Stream`].
///
/// [`TcpListener`]: struct@tokio_util::task::TaskSet
/// [`Stream`]: trait@crate::Stream
#[derive(Debug, Default)]
#[cfg_attr(docsrs, doc(cfg(feature = "task")))]
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
    type Item = Result<T, JoinError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_finished(cx)
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
