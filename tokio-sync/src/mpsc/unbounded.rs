use super::chan;

use loom::sync::atomic::AtomicUsize;
use futures::{Poll, Sink, StartSend, Stream};

use std::fmt;

#[derive(Clone)]
pub struct UnboundedSender<T> {
    chan: chan::Tx<T, Semaphore>,
}

/// TODO: Dox
pub struct UnboundedReceiver<T> {
    /// The channel receiver
    chan: chan::Rx<T, Semaphore>,
}

#[derive(Debug)]
pub struct UnboundedSendError {}

#[derive(Debug)]
pub struct UnboundedTrySendError<T>(T);

#[derive(Debug)]
pub struct UnboundedRecvError {}

/// TODO: Dox
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

    pub fn close(&mut self) {
        self.chan.close();
    }
}

impl<T> Stream for UnboundedReceiver<T> {
    type Item = T;
    type Error = UnboundedRecvError;

    fn poll(&mut self) -> Poll<Option<T>, Self::Error> {
        self.chan.recv()
            .map_err(|_| UnboundedRecvError {})
    }
}


impl<T> UnboundedSender<T> {
    pub(crate) fn new(chan: chan::Tx<T, Semaphore>) -> UnboundedSender<T> {
        UnboundedSender { chan }
    }

    /// Attempts to send a message on this `UnboundedSender` without blocking.
    pub fn try_send(&mut self, message: T)
        -> Result<(), UnboundedTrySendError<T>>
    {
        self.chan.try_send(message)?;
        Ok(())
    }
}

impl<T> Sink for UnboundedSender<T> {
    type SinkItem = T;
    type SinkError = UnboundedSendError;

    fn start_send(&mut self, msg: T) -> StartSend<T, Self::SinkError> {
        use futures::AsyncSink;

        self.try_send(msg).map_err(|_| UnboundedSendError {})?;
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
