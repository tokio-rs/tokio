use crate::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc::Receiver;

/// A wrapper around [`tokio::sync::mpsc::Receiver`] that implements [`Stream`].
///
/// # Example
///
/// ```
/// use tokio::sync::mpsc;
/// use tokio_stream::wrappers::ReceiverStream;
/// use tokio_stream::StreamExt;
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> Result<(), tokio::sync::mpsc::error::SendError<u8>> {
/// let (tx, rx) = mpsc::channel(2);
/// tx.send(10).await?;
/// tx.send(20).await?;
/// # // prevent the doc test from hanging
/// drop(tx);
///
/// let mut stream = ReceiverStream::new(rx);
/// assert_eq!(stream.next().await, Some(10));
/// assert_eq!(stream.next().await, Some(20));
/// assert_eq!(stream.next().await, None);
/// # Ok(())
/// # }
/// ```
///
/// [`tokio::sync::mpsc::Receiver`]: struct@tokio::sync::mpsc::Receiver
/// [`Stream`]: trait@crate::Stream
#[derive(Debug)]
pub struct ReceiverStream<T> {
    inner: Receiver<T>,
}

impl<T> ReceiverStream<T> {
    /// Create a new `ReceiverStream`.
    pub fn new(recv: Receiver<T>) -> Self {
        Self { inner: recv }
    }

    /// Get back the inner `Receiver`.
    pub fn into_inner(self) -> Receiver<T> {
        self.inner
    }

    /// Closes the receiving half of a channel without dropping it.
    ///
    /// This prevents any further messages from being sent on the channel while
    /// still enabling the receiver to drain messages that are buffered. Any
    /// outstanding [`Permit`] values will still be able to send messages.
    ///
    /// To guarantee no messages are dropped, after calling `close()`, you must
    /// receive all items from the stream until `None` is returned.
    ///
    /// [`Permit`]: struct@tokio::sync::mpsc::Permit
    pub fn close(&mut self) {
        self.inner.close();
    }
}

impl<T> Stream for ReceiverStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_recv(cx)
    }

    /// Returns the bounds of the stream based on the underlying receiver.
    ///
    /// For open channels, it returns `(receiver.len(), None)`.
    ///
    /// For closed channels, it returns `(receiver.len(), Some(used_capacity))`
    /// where `used_capacity` is calculated as `receiver.max_capacity() -
    /// receiver.capacity()`. This accounts for any [`Permit`] that is still
    /// able to send a message.
    ///
    /// [`Permit`]: struct@tokio::sync::mpsc::Permit
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.inner.is_closed() {
            let used_capacity = self.inner.max_capacity() - self.inner.capacity();
            (self.inner.len(), Some(used_capacity))
        } else {
            (self.inner.len(), None)
        }
    }
}

impl<T> AsRef<Receiver<T>> for ReceiverStream<T> {
    fn as_ref(&self) -> &Receiver<T> {
        &self.inner
    }
}

impl<T> AsMut<Receiver<T>> for ReceiverStream<T> {
    fn as_mut(&mut self) -> &mut Receiver<T> {
        &mut self.inner
    }
}

impl<T> From<Receiver<T>> for ReceiverStream<T> {
    fn from(recv: Receiver<T>) -> Self {
        Self::new(recv)
    }
}
