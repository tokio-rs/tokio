use super::chan;
use crate::loom::sync::atomic::AtomicUsize;

use std::fmt;
use std::task::{Context, Poll};

#[cfg(feature = "async-traits")]
use std::pin::Pin;

/// Send values to the associated `UnboundedReceiver`.
///
/// Instances are created by the
/// [`unbounded_channel`](fn.unbounded_channel.html) function.
pub struct UnboundedSender<T> {
    chan: chan::Tx<T, Semaphore>,
}

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        UnboundedSender {
            chan: self.chan.clone(),
        }
    }
}

impl<T> fmt::Debug for UnboundedSender<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("UnboundedSender")
            .field("chan", &self.chan)
            .finish()
    }
}

/// Receive values from the associated `UnboundedSender`.
///
/// Instances are created by the
/// [`unbounded_channel`](fn.unbounded_channel.html) function.
pub struct UnboundedReceiver<T> {
    /// The channel receiver
    chan: chan::Rx<T, Semaphore>,
}

impl<T> fmt::Debug for UnboundedReceiver<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("UnboundedReceiver")
            .field("chan", &self.chan)
            .finish()
    }
}

/// Error returned by the `UnboundedSender`.
#[derive(Debug)]
pub struct UnboundedSendError(());

/// Returned by `UnboundedSender::try_send` when the channel has been closed.
#[derive(Debug)]
pub struct UnboundedTrySendError<T>(T);

/// Error returned by `UnboundedReceiver`.
#[derive(Debug)]
pub struct UnboundedRecvError(());

/// Create an unbounded mpsc channel for communicating between asynchronous
/// tasks.
///
/// A `send` on this channel will always succeed as long as the receive half has
/// not been closed. If the receiver falls behind, messages will be arbitrarily
/// buffered.
///
/// **Note** that the amount of available system memory is an implicit bound to
/// the channel. Using an `unbounded` channel has the ability of causing the
/// process to run out of memory. In this case, the process will be aborted.
pub fn unbounded_channel<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let (tx, rx) = chan::channel(AtomicUsize::new(0));

    let tx = UnboundedSender::new(tx);
    let rx = UnboundedReceiver::new(rx);

    (tx, rx)
}

/// No capacity
type Semaphore = AtomicUsize;

impl<T> UnboundedReceiver<T> {
    pub(crate) fn new(chan: chan::Rx<T, Semaphore>) -> UnboundedReceiver<T> {
        UnboundedReceiver { chan }
    }

    /// TODO: dox
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.chan.recv(cx)
    }

    /// TODO: Dox
    #[allow(clippy::needless_lifetimes)] // false positive: https://github.com/rust-lang/rust-clippy/issues/3988
    pub async fn recv(&mut self) -> Option<T> {
        use futures_util::future::poll_fn;

        poll_fn(|cx| self.poll_recv(cx)).await
    }

    /// Closes the receiving half of a channel, without dropping it.
    ///
    /// This prevents any further messages from being sent on the channel while
    /// still enabling the receiver to drain messages that are buffered.
    pub fn close(&mut self) {
        self.chan.close();
    }
}

#[cfg(feature = "async-traits")]
impl<T> futures_core::Stream for UnboundedReceiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.chan.recv(cx)
    }
}

impl<T> UnboundedSender<T> {
    pub(crate) fn new(chan: chan::Tx<T, Semaphore>) -> UnboundedSender<T> {
        UnboundedSender { chan }
    }

    /// Attempts to send a message on this `UnboundedSender` without blocking.
    pub fn try_send(&mut self, message: T) -> Result<(), UnboundedTrySendError<T>> {
        self.chan.try_send(message)?;
        Ok(())
    }
}

#[cfg(feature = "async-traits")]
impl<T> futures_sink::Sink<T> for UnboundedSender<T> {
    type Error = UnboundedSendError;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, msg: T) -> Result<(), Self::Error> {
        self.try_send(msg).map_err(|_| UnboundedSendError(()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

// ===== impl UnboundedSendError =====

impl fmt::Display for UnboundedSendError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "channel closed")
    }
}

impl ::std::error::Error for UnboundedSendError {}

// ===== impl TrySendError =====

impl<T> UnboundedTrySendError<T> {
    /// Get the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: fmt::Debug> fmt::Display for UnboundedTrySendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "channel closed")
    }
}

impl<T: fmt::Debug> ::std::error::Error for UnboundedTrySendError<T> {}

impl<T> From<(T, chan::TrySendError)> for UnboundedTrySendError<T> {
    fn from((value, err): (T, chan::TrySendError)) -> UnboundedTrySendError<T> {
        assert_eq!(chan::TrySendError::Closed, err);
        UnboundedTrySendError(value)
    }
}

// ===== impl UnboundedRecvError =====

impl fmt::Display for UnboundedRecvError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "channel closed")
    }
}

impl ::std::error::Error for UnboundedRecvError {}
