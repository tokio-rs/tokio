use super::chan;
use semaphore::Permit;

use futures::{Sink, StartSend, Poll};

use std::sync::Arc;

pub struct Sender<T> {
    chan: chan::Tx<T>,
}

/// Error type for sending, used when the receiving end of a channel is
/// dropped
#[derive(Clone, PartialEq, Eq)]
pub struct SendError<T>(T);

impl<T> Sender<T> {
    pub(crate) fn new(chan: chan::Tx<T>) -> Sender<T> {
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
        /*
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
        */
        unimplemented!();
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        /*
        use futures::Async::Ready;
        Ok(Ready(()))
        */
        unimplemented!();
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        /*
        use futures::Async::Ready;
        Ok(Ready(()))
        */
        unimplemented!();
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        /*
        if self.state.is_reserved() {
            self.chan.release_capacity();
        }

        self.chan.ref_dec();
        */
        unimplemented!();
    }
}
