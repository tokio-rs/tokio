//! In-memory evented channels.
//!
//! This module contains a `Sender` and `Receiver` pair types which can be used
//! to send messages between different future tasks.

use std::io;
use std::sync::mpsc::TryRecvError;

use futures::{Future, Poll, Async};
use futures::stream::Stream;
use mio::channel;

use io::IoFuture;
use reactor::{Handle, PollEvented};

/// The transmission half of a channel used for sending messages to a receiver.
///
/// A `Sender` can be `clone`d to have multiple threads or instances sending
/// messages to one receiver.
///
/// This type is created by the `channel` function.
pub struct Sender<T> {
    tx: channel::Sender<T>,
}

/// The receiving half of a channel used for processing messages sent by a
/// `Sender`.
///
/// A `Receiver` cannot be cloned, so only one thread can receive messages at a
/// time.
///
/// This type is created by the `channel` function and implements the `Stream`
/// trait to represent received messages.
pub struct Receiver<T> {
    rx: PollEvented<channel::Receiver<T>>,
}

/// Future returned by the `channel` function which will resolve to a
/// `Receiver<T>`.
pub struct ReceiverNew<T> {
    inner: IoFuture<Receiver<T>>,
}

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
pub fn channel<T>(handle: &Handle) -> (Sender<T>, ReceiverNew<T>)
    where T: Send + 'static,
{
    let (tx, rx) = channel::channel();
    let rx = PollEvented::new(rx, handle).map(|rx| Receiver { rx: rx });
    (Sender { tx: tx }, ReceiverNew { inner: rx.boxed() })
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
        if let Async::NotReady = self.rx.poll_read() {
            return Ok(Async::NotReady)
        }
        match self.rx.get_ref().try_recv() {
            Ok(t) => Ok(Async::Ready(Some(t))),
            Err(TryRecvError::Empty) => {
                self.rx.need_read();
                Ok(Async::NotReady)
            }
            Err(TryRecvError::Disconnected) => Ok(Async::Ready(None)),
        }
    }
}

impl<T> Future for ReceiverNew<T> {
    type Item = Receiver<T>;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Receiver<T>, io::Error> {
        self.inner.poll()
    }
}
