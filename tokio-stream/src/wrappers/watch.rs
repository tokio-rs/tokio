use std::pin::Pin;
use tokio::sync::watch::Receiver;

use futures_core::Stream;
use tokio_util::sync::ReusableBoxFuture;

use std::fmt;
use std::task::{Context, Poll};
use tokio::sync::watch::error::RecvError;

/// A wrapper around [`tokio::sync::watch::Receiver`] that implements [`Stream`].
///
/// [`tokio::sync::watch::Receiver`]: struct@tokio::sync::watch::Receiver
/// [`Stream`]: trait@crate::Stream
pub struct WatchStream<T> {
    inner: ReusableBoxFuture<Result<((), Receiver<T>), RecvError>>,
}

async fn make_future<T: Clone + Send + Sync>(
    mut rx: Receiver<T>,
) -> Result<((), Receiver<T>), RecvError> {
    let signal = rx.changed().await?;
    Ok((signal, rx))
}

impl<T: 'static + Clone + Unpin + Send + Sync> WatchStream<T> {
    /// Create a new `WatchStream`.
    pub fn new(rx: Receiver<T>) -> Self {
        Self {
            inner: ReusableBoxFuture::new(make_future(rx)),
        }
    }
}

impl<T: Clone + 'static + Send + Sync> Stream for WatchStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match ready!(self.inner.poll(cx)) {
            Ok((_, rx)) => {
                let received = (*rx.borrow()).clone();
                self.inner.set(make_future(rx));
                Poll::Ready(Some(received))
            }
            Err(_) => Poll::Ready(None),
        }
    }
}

impl<T> Unpin for WatchStream<T> {}

impl<T> fmt::Debug for WatchStream<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WatchStream").finish()
    }
}
