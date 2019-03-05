use super::chan;

use futures::{Poll, Sink, StartSend, Stream};
use loom::sync::atomic::AtomicUsize;

use std::fmt;

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
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
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
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
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

    /// Closes the receiving half of a channel, without dropping it.
    ///
    /// This prevents any further messages from being sent on the channel while
    /// still enabling the receiver to drain messages that are buffered.
    pub fn close(&mut self) {
        self.chan.close();
    }
}

impl<T> Stream for UnboundedReceiver<T> {
    type Item = T;
    type Error = UnboundedRecvError;

    fn poll(&mut self) -> Poll<Option<T>, Self::Error> {
        self.chan.recv().map_err(|_| UnboundedRecvError(()))
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

impl<T> Sink for UnboundedSender<T> {
    type SinkItem = T;
    type SinkError = UnboundedSendError;

    fn start_send(&mut self, msg: T) -> StartSend<T, Self::SinkError> {
        use futures::AsyncSink;

        self.try_send(msg).map_err(|_| UnboundedSendError(()))?;
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        use futures::Async::Ready;
        Ok(Ready(()))
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        use futures::Async::Ready;
        Ok(Ready(()))
    }
}

// ===== impl UnboundedSendError =====

impl fmt::Display for UnboundedSendError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use std::error::Error;
        write!(fmt, "{}", self.description())
    }
}

impl ::std::error::Error for UnboundedSendError {
    fn description(&self) -> &str {
        "channel closed"
    }
}

// ===== impl TrySendError =====

impl<T> UnboundedTrySendError<T> {
    /// Get the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: fmt::Debug> fmt::Display for UnboundedTrySendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use std::error::Error;
        write!(fmt, "{}", self.description())
    }
}

impl<T: fmt::Debug> ::std::error::Error for UnboundedTrySendError<T> {
    fn description(&self) -> &str {
        "channel closed"
    }
}

impl<T> From<(T, chan::TrySendError)> for UnboundedTrySendError<T> {
    fn from((value, err): (T, chan::TrySendError)) -> UnboundedTrySendError<T> {
        assert_eq!(chan::TrySendError::Closed, err);
        UnboundedTrySendError(value)
    }
}

// ===== impl UnboundedRecvError =====

impl fmt::Display for UnboundedRecvError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use std::error::Error;
        write!(fmt, "{}", self.description())
    }
}

impl ::std::error::Error for UnboundedRecvError {
    fn description(&self) -> &str {
        "channel closed"
    }
}
