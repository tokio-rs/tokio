use super::chan;

use futures::{Poll, Sink, StartSend, Stream};

use std::fmt;

/// Send values to the associated `Receiver`.
///
/// Instances are created by the [`channel`](channel) function.
#[derive(Debug, Clone)]
pub struct Sender<T> {
    chan: chan::Tx<T, Semaphore>,
}

/// Receive values from the associated `Sender`.
///
/// Instances are created by the [`channel`](channel) function.
#[derive(Debug)]
pub struct Receiver<T> {
    /// The channel receiver
    chan: chan::Rx<T, Semaphore>,
}

/// Error returned by the `Sender`.
#[derive(Debug)]
pub struct SendError(());

/// Error returned by `Sender::try_send`.
#[derive(Debug)]
pub struct TrySendError<T> {
    kind: ErrorKind,
    value: T,
}

#[derive(Debug)]
enum ErrorKind {
    Closed,
    NoCapacity,
}

/// Error returned by the `Receiver`.
#[derive(Debug)]
pub struct RecvError {}

/// Create a bounded mpsc channel for communicating between asynchronous tasks.
pub fn channel<T>(buffer: usize) -> (Sender<T>, Receiver<T>) {
    let semaphore = (::semaphore::Semaphore::new(buffer), buffer);
    let (tx, rx) = chan::channel(semaphore);

    let tx = Sender::new(tx);
    let rx = Receiver::new(rx);

    (tx, rx)
}

/// Channel semaphore is a tuple of the semaphore implementation and a `usize`
/// representing the channel bound.
type Semaphore = (::semaphore::Semaphore, usize);

impl<T> Receiver<T> {
    pub(crate) fn new(chan: chan::Rx<T, Semaphore>) -> Receiver<T> {
        Receiver { chan }
    }

    /// Closes the receiving half of a channel, without dropping it.
    ///
    /// This prevents any further messages from being sent on the channel while
    /// still enabling the receiver to drain messages that are buffered.
    pub fn close(&mut self) {
        self.chan.close();
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<T>, Self::Error> {
        self.chan.recv()
    }
}


impl<T> Sender<T> {
    pub(crate) fn new(chan: chan::Tx<T, Semaphore>) -> Sender<T> {
        Sender { chan }
    }

    pub fn poll_ready(&mut self) -> Poll<(), SendError> {
        self.chan.poll_ready()
            .map_err(|_| SendError(()))
    }

    /// Attempts to send a message on this `Sender`, returning the message
    /// if there was an error.
    pub fn try_send(&mut self, message: T) -> Result<(), TrySendError<T>> {
        self.chan.try_send(message)?;
        Ok(())
    }
}

impl<T> Sink for Sender<T> {
    type SinkItem = T;
    type SinkError = SendError;

    fn start_send(&mut self, msg: T) -> StartSend<T, Self::SinkError> {
        use futures::AsyncSink;
        use futures::Async::*;

        match self.poll_ready() {
            Ok(Ready(_)) => {
                self.try_send(msg).map_err(|_| SendError(()))?;
                Ok(AsyncSink::Ready)
            }
            Ok(NotReady) => {
                Ok(AsyncSink::NotReady(msg))
            }
            Err(e) => Err(e),
        }
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

// ===== impl SendError =====

impl fmt::Display for SendError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use std::error::Error;
        write!(fmt, "{}", self.description())
    }
}

impl ::std::error::Error for SendError {
    fn description(&self) -> &str {
        "channel closed"
    }
}

// ===== impl TrySendError =====

impl<T: fmt::Debug> fmt::Display for TrySendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use std::error::Error;
        write!(fmt, "{}", self.description())
    }
}

impl<T: fmt::Debug> ::std::error::Error for TrySendError<T> {
    fn description(&self) -> &str {
        match self.kind {
            ErrorKind::Closed => "channel closed",
            ErrorKind::NoCapacity => "no available capacity",
        }
    }
}

impl<T> From<(T, chan::TrySendError)> for TrySendError<T> {
    fn from((value, err): (T, chan::TrySendError)) -> TrySendError<T> {
        TrySendError {
            value,
            kind: match err {
                chan::TrySendError::Closed => ErrorKind::Closed,
                chan::TrySendError::NoPermits => ErrorKind::NoCapacity,
            }
        }
    }
}
