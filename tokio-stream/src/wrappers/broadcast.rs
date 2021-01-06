use crate::Stream;
use async_stream::try_stream;
use std::pin::Pin;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;

use std::task::{Context, Poll};

/// A wrapper around [`Receiver`] that implements [`Stream`]. Achieved by using the [`async-stream`] crate.
///
/// [`Receiver`]: struct@tokio::sync::broadcast::Receiver
/// [`Stream`]: trait@crate::Stream
/// [`async-stream`]: https://docs.rs/async-stream
#[cfg_attr(docsrs, doc(cfg(feature = "net")))]
pub struct BroadcastStream<T: Clone> {
    inner: Pin<Box<dyn Stream<Item = Result<T, RecvError>>>>,
}

impl<T: Clone + Unpin + 'static> BroadcastStream<T> {
    /// Create a new `BroadcastStream`.
    pub fn new(mut rx: Receiver<T>) -> Self {
        let stream = try_stream! {
            loop {
                let item = rx.recv().await?;
                yield item;
            }
        };
        Self { inner: Box::pin(stream) }
    }
}

impl<T: Clone> Stream for BroadcastStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next(cx)
    }
}
