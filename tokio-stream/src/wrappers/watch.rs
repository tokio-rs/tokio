use std::pin::Pin;
use tokio::sync::watch::Receiver;

use futures_core::Stream;
use tokio_util::sync::ReusableBoxFuture;

use std::fmt;
use std::task::{Context, Poll};
use tokio::sync::watch::error::RecvError;

/// A wrapper around [`tokio::sync::watch::Receiver`] that implements [`Stream`].
///
/// This stream will always start by yielding the value present the Receiver when the WatchStream
/// is constructed. As such, you are advised to construct the WatchStream before using the Sender.
/// If you don't, you may receive the current value twice.
///
/// # Examples
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use tokio_stream::{StreamExt, wrappers::WatchStream};
/// use tokio::sync::watch;
///
/// let (tx, rx) = watch::channel("hello");
///
/// let mut rx = WatchStream::new(rx);
/// tx.send("goodbye").unwrap();
///
/// assert_eq!(rx.next().await, Some("hello"));
/// assert_eq!(rx.next().await, Some("goodbye"));
/// # }
/// ```
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use tokio_stream::{StreamExt, wrappers::WatchStream};
/// use tokio::sync::watch;
///
/// let (tx, rx) = watch::channel("hello");
///
/// // NOT RECOMMENDED!
/// tx.send("goodbye").unwrap();
/// let mut rx = WatchStream::new(rx);
///
/// tokio::task::spawn(async move {
///     tokio::time::sleep(std::time::Duration::from_millis(10)).await;
///     tx.send("hello again").unwrap();
/// });
///
/// // goodbye will be received twice
/// assert_eq!(rx.next().await, Some("goodbye"));
/// assert_eq!(rx.next().await, Some("goodbye"));
/// assert_eq!(rx.next().await, Some("hello again"));
/// # }
/// ```
///
/// [`tokio::sync::watch::Receiver`]: struct@tokio::sync::watch::Receiver
/// [`Stream`]: trait@crate::Stream
#[cfg_attr(docsrs, doc(cfg(feature = "sync")))]
pub struct WatchStream<T> {
    inner: ReusableBoxFuture<(Result<T, RecvError>, Receiver<T>)>,
}

async fn make_future<T: Clone + Send + Sync>(
    mut rx: Receiver<T>,
) -> (Result<T, RecvError>, Receiver<T>) {
    let result = rx.changed().await;
    let result = result.map(|()| (*rx.borrow()).clone());
    (result, rx)
}

impl<T: 'static + Clone + Unpin + Send + Sync> WatchStream<T> {
    /// Create a new `WatchStream`.
    pub fn new(rx: Receiver<T>) -> Self {
        let initial = (*rx.borrow()).clone();

        Self {
            inner: ReusableBoxFuture::new(async move { (Ok(initial), rx) }),
        }
    }
}

impl<T: Clone + 'static + Send + Sync> Stream for WatchStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (result, rx) = ready!(self.inner.poll(cx));
        match result {
            Ok(received) => {
                self.inner.set(make_future(rx));
                Poll::Ready(Some(received))
            }
            Err(_) => {
                self.inner.set(make_future(rx));
                Poll::Ready(None)
            }
        }
    }
}

impl<T> Unpin for WatchStream<T> {}

impl<T> fmt::Debug for WatchStream<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WatchStream").finish()
    }
}
