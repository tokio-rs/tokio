use crate::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc::UnboundedReceiver;

/// A wrapper around [`tokio::sync::mpsc::UnboundedReceiver`] that implements [`Stream`].
///
/// # Example
///
/// ```
/// use tokio::sync::mpsc;
/// use tokio_stream::wrappers::UnboundedReceiverStream;
/// use tokio_stream::StreamExt;
///
/// #[tokio::main]
/// async fn main() {
///     let (tx, rx) = mpsc::unbounded_channel();
///     tx.send(10).unwrap();
///     tx.send(20).unwrap();
///     drop(tx);
///
///     let mut stream = UnboundedReceiverStream::new(rx);
///     assert_eq!(stream.next().await, Some(10));
///     assert_eq!(stream.next().await, Some(20));
///     assert_eq!(stream.next().await, None);
/// }
/// ```
///
/// [`tokio::sync::mpsc::UnboundedReceiver`]: struct@tokio::sync::mpsc::UnboundedReceiver
/// [`Stream`]: trait@crate::Stream
#[derive(Debug)]
pub struct UnboundedReceiverStream<T> {
    inner: UnboundedReceiver<T>,
}

impl<T> UnboundedReceiverStream<T> {
    /// Create a new `UnboundedReceiverStream`.
    pub fn new(recv: UnboundedReceiver<T>) -> Self {
        Self { inner: recv }
    }

    /// Get back the inner `UnboundedReceiver`.
    pub fn into_inner(self) -> UnboundedReceiver<T> {
        self.inner
    }

    /// Closes the receiving half of a channel without dropping it.
    ///
    /// This prevents any further messages from being sent on the channel while
    /// still enabling the receiver to drain messages that are buffered.
    pub fn close(&mut self) {
        self.inner.close();
    }
}

impl<T> Stream for UnboundedReceiverStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_recv(cx)
    }
}

impl<T> AsRef<UnboundedReceiver<T>> for UnboundedReceiverStream<T> {
    fn as_ref(&self) -> &UnboundedReceiver<T> {
        &self.inner
    }
}

impl<T> AsMut<UnboundedReceiver<T>> for UnboundedReceiverStream<T> {
    fn as_mut(&mut self) -> &mut UnboundedReceiver<T> {
        &mut self.inner
    }
}

impl<T> From<UnboundedReceiver<T>> for UnboundedReceiverStream<T> {
    fn from(recv: UnboundedReceiver<T>) -> Self {
        Self::new(recv)
    }
}
