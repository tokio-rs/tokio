//! A thin wrapper around a mpsc queue and mio-based channel information
//!
//! Normally the standard library's channels would suffice but we unfortunately
//! need the `Sender<T>` half to be `Sync`, so to accomplish this for now we
//! just vendor the same mpsc queue as the one in the standard library and then
//! we pair that with the `mio::channel` module's Ctl pairs to control the
//! readiness notifications on the channel.

use std::cell::Cell;
use std::io;
use std::marker;
use std::sync::Arc;

use mio;
use mio::channel::{ctl_pair, SenderCtl, ReceiverCtl};

use mpsc_queue::{Queue, PopResult};

pub struct Sender<T> {
    ctl: SenderCtl,
    inner: Arc<Queue<T>>,
}

pub struct Receiver<T> {
    ctl: ReceiverCtl,
    inner: Arc<Queue<T>>,
    _marker: marker::PhantomData<Cell<()>>, // this type is not Sync
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Queue::new());
    let (tx, rx) = ctl_pair();

    let tx = Sender {
        ctl: tx,
        inner: inner.clone(),
    };
    let rx = Receiver {
        ctl: rx,
        inner: inner.clone(),
        _marker: marker::PhantomData,
    };
    (tx, rx)
}

impl<T> Sender<T> {
    pub fn send(&self, data: T) -> io::Result<()> {
        self.inner.push(data);
        self.ctl.inc()
    }
}

impl<T> Receiver<T> {
    pub fn recv(&self) -> io::Result<Option<T>> {
        // Note that the underlying method is `unsafe` because it's only safe
        // if one thread accesses it at a time.
        //
        // We, however, are the only thread with a `Receiver<T>` because this
        // type is not `Sync`. and we never handed out another instance.
        match unsafe { self.inner.pop() } {
            PopResult::Data(t) => {
                try!(self.ctl.dec());
                Ok(Some(t))
            }

            // If the queue is either in an inconsistent or empty state, then
            // we return `None` for both instances. Note that the standard
            // library performs a yield loop in the event of `Inconsistent`,
            // which means that there's data in the queue but a sender hasn't
            // finished their operation yet.
            //
            // We do this because the queue will continue to be readable as
            // the thread performing the push will eventually call `inc`, so
            // if we return `None` and the event loop just loops aruond calling
            // this method then we'll eventually get back to the same spot
            // and due the retry.
            //
            // Basically, the inconsistent state doesn't mean we need to busy
            // wait, but instead we can forge ahead and assume by the time we
            // go to the kernel and come back we'll no longer be in an
            // inconsistent state.
            PopResult::Empty |
            PopResult::Inconsistent => Ok(None),
        }
    }
}

// Just delegate everything to `self.ctl`
impl<T> mio::Evented for Receiver<T> {
    fn register(&self,
                poll: &mio::Poll,
                token: mio::Token,
                interest: mio::Ready,
                opts: mio::PollOpt) -> io::Result<()> {
        self.ctl.register(poll, token, interest, opts)
    }

    fn reregister(&self,
                  poll: &mio::Poll,
                  token: mio::Token,
                  interest: mio::Ready,
                  opts: mio::PollOpt) -> io::Result<()> {
        self.ctl.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        self.ctl.deregister(poll)
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Sender<T> {
        Sender {
            ctl: self.ctl.clone(),
            inner: self.inner.clone(),
        }
    }
}
