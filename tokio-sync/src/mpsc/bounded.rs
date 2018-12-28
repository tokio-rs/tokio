use super::chan;

use futures::{Poll, Sink, StartSend, Stream};

#[derive(Clone)]
pub struct Sender<T> {
    chan: chan::Tx<T, Semaphore>,
}

/// TODO: Dox
pub struct Receiver<T> {
    /// The channel receiver
    chan: chan::Rx<T, Semaphore>,
}

/// Error type for sending, used when the receiving end of a channel is
/// dropped
#[derive(Clone, PartialEq, Eq)]
pub struct SendError<T>(T);

/// TODO: Dox
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

    pub fn poll_ready(&mut self) -> Poll<(), ()> {
        self.chan.poll_ready()
    }

    /// Attempts to send a message on this `Sender` without blocking.
    pub fn try_send(&mut self, message: T) -> Result<(), ()> {
        self.chan.try_send(message)
    }
}

impl<T> Sink for Sender<T> {
    type SinkItem = T;
    type SinkError = ();

    fn start_send(&mut self, msg: T) -> StartSend<T, Self::SinkError> {
        use futures::AsyncSink;
        use futures::Async::*;

        match self.poll_ready() {
            Ok(Ready(_)) => {
                self.try_send(msg)?;
                Ok(AsyncSink::Ready)
            }
            Ok(NotReady) => {
                Ok(AsyncSink::NotReady(msg))
            }
            Err(e) => Err(e),
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.poll_ready()
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        use futures::Async::Ready;
        Ok(Ready(()))
    }
}
