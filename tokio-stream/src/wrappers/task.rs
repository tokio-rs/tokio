use crate::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::task::{JoinError, JoinSet};

/// A wrapper around [`tokio::task::JoinSet`] that implements [`Stream`].
///
/// # Example
///
/// ```
/// use tokio::task::JoinSet;
/// use tokio_stream::wrappers::JoinSetStream;
/// use tokio_stream::StreamExt;
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> Result<(), tokio::task::JoinError> {
/// let set: JoinSet<_> = (0..2).map(|i| async move { i }).collect();
///
/// let mut stream = JoinSetStream::new(set);
/// assert_eq!(stream.next().await.transpose()?, Some(0));
/// assert_eq!(stream.next().await.transpose()?, Some(1));
/// assert_eq!(stream.next().await.transpose()?, None);
/// # Ok(())
/// # }
/// ```
///
/// [`tokio::task::JoinSet`]: struct@tokio::task::JoinSet
/// [`Stream`]: trait@crate::Stream
#[derive(Debug)]
pub struct JoinSetStream<T> {
    inner: JoinSet<T>,
}

impl<T> JoinSetStream<T> {
    /// Create a new `JoinSetStream`.
    pub fn new(join_set: JoinSet<T>) -> Self {
        Self { inner: join_set }
    }

    /// Get back the inner `JoinSet`.
    pub fn into_inner(self) -> JoinSet<T> {
        self.inner
    }
}

impl<T: 'static> Stream for JoinSetStream<T> {
    type Item = Result<T, JoinError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_join_next(cx)
    }

    /// Returns the bounds of the stream based on the underlying `JoinSet`.
    ///
    /// It returns `(set.len(), Some(set.len()))`.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.inner.len();
        (size, Some(size))
    }
}

impl<T> AsRef<JoinSet<T>> for JoinSetStream<T> {
    fn as_ref(&self) -> &JoinSet<T> {
        &self.inner
    }
}

impl<T> AsMut<JoinSet<T>> for JoinSetStream<T> {
    fn as_mut(&mut self) -> &mut JoinSet<T> {
        &mut self.inner
    }
}

impl<T> From<JoinSet<T>> for JoinSetStream<T> {
    fn from(join_set: JoinSet<T>) -> Self {
        Self::new(join_set)
    }
}
