use crate::Stream;
use async_stream::stream;
use std::pin::Pin;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;

use std::fmt;
use std::task::{Context, Poll};

/// A wrapper around [`Receiver`] that implements [`Stream`].
///
/// [`Receiver`]: struct@tokio::sync::broadcast::Receiver
/// [`Stream`]: trait@crate::Stream
pub struct BroadcastStream<T> {
    inner: Pin<Box<dyn Stream<Item = Result<T, BroadcastStreamRecvError>> + Send + Sync>>,
}

/// An error returned from the inner stream of a [`BroadcastStream`].
#[derive(Debug, PartialEq)]
pub enum BroadcastStreamRecvError {
    /// The receiver lagged too far behind. Attempting to receive again will
    /// return the oldest message still retained by the channel.
    ///
    /// Includes the number of skipped messages.
    Lagged(u64),
}

impl<T: Clone + Unpin + 'static + Send + Sync> BroadcastStream<T> {
    /// Create a new `BroadcastStream`.
    pub fn new(mut rx: Receiver<T>) -> Self {
        let stream = stream! {
            loop {
                match rx.recv().await {
                Ok(item) => yield Ok(item),
                Err(err) =>
                    match err {
                         RecvError::Closed => break,
                         RecvError::Lagged(n) =>
                             yield Err(BroadcastStreamRecvError::Lagged(n))
                    }
                }
            }
        };
        Self {
            inner: Box::pin(stream),
        }
    }
}

impl<T: Clone> Stream for BroadcastStream<T> {
    type Item = Result<T, BroadcastStreamRecvError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl<T: Clone> fmt::Debug for BroadcastStream<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BroadcastStream").finish()
    }
}
