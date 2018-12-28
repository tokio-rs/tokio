use super::chan;

use loom::sync::atomic::AtomicUsize;

use futures::{Poll, Sink, StartSend, Stream};

#[derive(Clone)]
pub struct UnboundedSender<T> {
    chan: chan::Tx<T, Semaphore>,
}

/// TODO: Dox
pub struct UnboundedReceiver<T> {
    /// The channel receiver
    chan: chan::Rx<T, Semaphore>,
}

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
    type Error = ();

    fn poll(&mut self) -> Poll<Option<T>, Self::Error> {
        self.chan.recv()
    }
}


impl<T> UnboundedSender<T> {
    pub(crate) fn new(chan: chan::Tx<T, Semaphore>) -> UnboundedSender<T> {
        UnboundedSender { chan }
    }

    /// Attempts to send a message on this `UnboundedSender` without blocking.
    pub fn try_send(&mut self, message: T) -> Result<(), ()> {
        self.chan.try_send(message)
    }
}

impl<T> Sink for UnboundedSender<T> {
    type SinkItem = T;
    type SinkError = ();

    fn start_send(&mut self, msg: T) -> StartSend<T, Self::SinkError> {
        use futures::AsyncSink;

        self.try_send(msg)?;
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
