use crate::Stream;
use async_stream::stream;
use std::pin::Pin;
use tokio::sync::watch::Receiver;

use std::fmt;
use std::task::{Context, Poll};

/// A wrapper around [`tokio::sync::watch::Receiver`] that implements [`Stream`].
///
/// [`tokio::sync::watch::Receiver`]: struct@tokio::sync::watch::Receiver
/// [`Stream`]: trait@crate::Stream
pub struct WatchStream<T> {
    inner: Pin<Box<dyn Stream<Item = T>>>,
}

impl<T: 'static + Clone + Unpin> WatchStream<T> {
    /// Create a new `WatchStream`.
    pub fn new(mut rx: Receiver<T>) -> Self {
        let stream = stream! {
            loop {
                match rx.changed().await {
                    Ok(_) => yield (*rx.borrow()).clone(),
                    Err(_) => break,
                }
            }
        };
        Self {
            inner: Box::pin(stream),
        }
    }
}

impl<T> Stream for WatchStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl<T> Unpin for WatchStream<T> {}

impl<T> fmt::Debug for WatchStream<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WatchStream").finish()
    }
}
