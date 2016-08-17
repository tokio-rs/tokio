use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::{Future, Poll};

use event_loop::{IoSource, LoopHandle, AddSource};

/// A concrete implementation of a stream of readiness notifications for I/O
/// objects that originates from an event loop.
///
/// Created by the `ReadinessStream::new` method, each `ReadinessStream` is
/// associated with a specific event loop and source of events that will be
/// registered with an event loop.
///
/// Currently readiness streams have "edge" semantics. That is, if a stream
/// receives a readable notification it will not receive another readable
/// notification until all bytes have been read from the stream.
///
/// Note that the precise semantics of when notifications are received will
/// likely be configurable in the future.
pub struct ReadinessStream {
    io_token: usize,
    loop_handle: LoopHandle,
    source: IoSource,
    readiness: AtomicUsize,
}

pub struct ReadinessStreamNew {
    inner: AddSource,
    handle: Option<LoopHandle>,
    source: Option<IoSource>,
}

impl ReadinessStream {
    /// Creates a new readiness stream associated with the provided
    /// `loop_handle` and for the given `source`.
    ///
    /// This method returns a future which will resolve to the readiness stream
    /// when it's ready.
    pub fn new(loop_handle: LoopHandle, source: IoSource)
               -> ReadinessStreamNew {
        ReadinessStreamNew {
            inner: loop_handle.add_source(source.clone()),
            source: Some(source),
            handle: Some(loop_handle),
        }
    }

    /// Tests to see if this source is ready to be read from or not.
    pub fn poll_read(&self) -> Poll<(), io::Error> {
        if self.readiness.load(Ordering::SeqCst) & 1 != 0 {
            return Poll::Ok(())
        }
        self.readiness.fetch_or(self.source.take_readiness(), Ordering::SeqCst);
        if self.readiness.load(Ordering::SeqCst) & 1 != 0 {
            Poll::Ok(())
        } else {
            self.loop_handle.schedule_read(self.io_token);
            Poll::NotReady
        }
    }

    /// Tests to see if this source is ready to be written to or not.
    pub fn poll_write(&self) -> Poll<(), io::Error> {
        if self.readiness.load(Ordering::SeqCst) & 2 != 0 {
            return Poll::Ok(())
        }
        self.readiness.fetch_or(self.source.take_readiness(), Ordering::SeqCst);
        if self.readiness.load(Ordering::SeqCst) & 2 != 0 {
            Poll::Ok(())
        } else {
            self.loop_handle.schedule_write(self.io_token);
            Poll::NotReady
        }
    }

    /// Tests to see if this source is ready to be read from or not.
    pub fn need_read(&self) {
        self.readiness.fetch_and(!1, Ordering::SeqCst);
        self.loop_handle.schedule_read(self.io_token);
    }

    /// Tests to see if this source is ready to be written to or not.
    pub fn need_write(&self) {
        self.readiness.fetch_and(!2, Ordering::SeqCst);
        self.loop_handle.schedule_write(self.io_token);
    }
}

impl Future for ReadinessStreamNew {
    type Item = ReadinessStream;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<ReadinessStream, io::Error> {
        self.inner.poll().map(|token| {
            ReadinessStream {
                io_token: token,
                source: self.source.take().unwrap(),
                loop_handle: self.handle.take().unwrap(),
                readiness: AtomicUsize::new(0),
            }
        })
    }
}

impl Drop for ReadinessStream {
    fn drop(&mut self) {
        self.loop_handle.drop_source(self.io_token)
    }
}
