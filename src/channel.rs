use std::io;
use std::sync::mpsc::TryRecvError;

use futures::{Future, Poll};
use futures::stream::Stream;
use mio::channel;

use {ReadinessStream, LoopHandle};
use io::IoFuture;

/// The transmission half of a channel used for sending messages to a receiver.
///
/// A `Sender` can be `clone`d to have multiple threads or instances sending
/// messages to one receiver.
///
/// This type is created by the `LoopHandle::channel` method.
pub struct Sender<T> {
    tx: channel::Sender<T>,
}

/// The receiving half of a channel used for processing messages sent by a
/// `Sender`.
///
/// A `Receiver` cannot be cloned, so only one thread can receive messages at a
/// time.
///
/// This type is created by the `LoopHandle::channel` method and implements the
/// `Stream` trait to represent received messages.
pub struct Receiver<T> {
    rx: ReadinessStream<channel::Receiver<T>>,
}

impl LoopHandle {
    /// Creates a new in-memory channel used for sending data across `Send +
    /// 'static` boundaries, frequently threads.
    ///
    /// This type can be used to conveniently send messages between futures.
    /// Unlike the futures crate `channel` method and types, the returned tx/rx
    /// pair is a multi-producer single-consumer (mpsc) channel *with no
    /// backpressure*. Currently it's left up to the application to implement a
    /// mechanism, if necessary, to avoid messages piling up.
    ///
    /// The returned `Sender` can be used to send messages that are processed by
    /// the returned `Receiver`. The `Sender` can be cloned to send messages
    /// from multiple sources simultaneously.
    pub fn channel<T>(self) -> (Sender<T>, IoFuture<Receiver<T>>)
        where T: Send + 'static,
    {
        let (tx, rx) = channel::channel();
        let rx = ReadinessStream::new(self, rx).map(|rx| Receiver { rx: rx });
        (Sender { tx: tx }, rx.boxed())
    }
}

impl<T> Sender<T> {
    /// Sends a message to the corresponding receiver of this sender.
    ///
    /// The message provided will be enqueued on the channel immediately, and
    /// this function will return immediately. Keep in mind that the
    /// underlying channel has infinite capacity, and this may not always be
    /// desired.
    ///
    /// If an I/O error happens while sending the message, or if the receiver
    /// has gone away, then an error will be returned. Note that I/O errors here
    /// are generally quite abnormal.
    pub fn send(&self, t: T) -> io::Result<()> {
        self.tx.send(t).map_err(|e| {
            match e {
                channel::SendError::Io(e) => e,
                channel::SendError::Disconnected(_) => {
                    io::Error::new(io::ErrorKind::Other,
                                   "channel has been disconnected")
                }
            }
        })
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Sender<T> {
        Sender { tx: self.tx.clone() }
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<T>, io::Error> {
        match self.rx.poll_read() {
            Poll::Ok(()) => {}
            _ => return Poll::NotReady,
        }
        match self.rx.get_ref().try_recv() {
            Ok(t) => Poll::Ok(Some(t)),
            Err(TryRecvError::Empty) => {
                self.rx.need_read();
                Poll::NotReady
            }
            Err(TryRecvError::Disconnected) => Poll::Ok(None),
        }
    }
}
