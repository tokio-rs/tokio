use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::{Future, Poll, Async};
use mio;

use event_loop::{IoToken, LoopHandle, AddSource};

/// A concrete implementation of a stream of readiness notifications for I/O
/// objects that originates from an event loop.
///
/// Created by the `ReadinessStream::new` method, each `ReadinessStream` is
/// associated with a specific event loop and source of events that will be
/// registered with an event loop.
///
/// Each readiness stream has a number of methods to test whether the underlying
/// object is readable or writable. Once the methods return that an object is
/// readable/writable, then it will continue to do so until the `need_read` or
/// `need_write` methods are called.
///
/// That is, this object is typically wrapped in another form of I/O object.
/// It's the responsibility of the wrapper to inform the readiness stream when a
/// "would block" I/O event is seen. The readiness stream will then take care of
/// any scheduling necessary to get notified when the event is ready again.
pub struct ReadinessStream<E> {
    token: IoToken,
    handle: LoopHandle,
    readiness: AtomicUsize,
    io: E,
}

pub struct ReadinessStreamNew<E> {
    inner: AddSource<E>,
    handle: LoopHandle,
}

impl<E> ReadinessStream<E>
    where E: mio::Evented + Send + 'static,
{
    /// Creates a new readiness stream associated with the provided
    /// `loop_handle` and for the given `source`.
    ///
    /// This method returns a future which will resolve to the readiness stream
    /// when it's ready.
    pub fn new(loop_handle: LoopHandle, source: E) -> ReadinessStreamNew<E> {
        ReadinessStreamNew {
            inner: loop_handle.add_source(source),
            handle: loop_handle,
        }
    }
}

impl<E> ReadinessStream<E> {
    /// Tests to see if this source is ready to be read from or not.
    ///
    /// If this stream is not ready for a read then `NotReady` will be returned
    /// and the current task will be scheduled to receive a notification when
    /// the stream is readable again. In other words, this method is only safe
    /// to call from within the context of a future's task, typically done in a
    /// `Future::poll` method.
    pub fn poll_read(&self) -> Poll<(), io::Error> {
        if self.readiness.load(Ordering::SeqCst) & 1 != 0 {
            return Ok(Async::Ready(()))
        }
        self.readiness.fetch_or(self.token.take_readiness(), Ordering::SeqCst);
        if self.readiness.load(Ordering::SeqCst) & 1 != 0 {
            Ok(Async::Ready(()))
        } else {
            self.handle.schedule_read(&self.token);
            Ok(Async::NotReady)
        }
    }

    /// Tests to see if this source is ready to be written to or not.
    ///
    /// If this stream is not ready for a write then `NotReady` will be returned
    /// and the current task will be scheduled to receive a notification when
    /// the stream is writable again. In other words, this method is only safe
    /// to call from within the context of a future's task, typically done in a
    /// `Future::poll` method.
    pub fn poll_write(&self) -> Poll<(), io::Error> {
        if self.readiness.load(Ordering::SeqCst) & 2 != 0 {
            return Ok(Async::Ready(()))
        }
        self.readiness.fetch_or(self.token.take_readiness(), Ordering::SeqCst);
        if self.readiness.load(Ordering::SeqCst) & 2 != 0 {
            Ok(Async::Ready(()))
        } else {
            self.handle.schedule_write(&self.token);
            Ok(Async::NotReady)
        }
    }

    /// Indicates to this source of events that the corresponding I/O object is
    /// no longer readable, but it needs to be.
    ///
    /// This function, like `poll_read`, is only safe to call from the context
    /// of a future's task (typically in a `Future::poll` implementation). It
    /// informs this readiness stream that the underlying object is no longer
    /// readable, typically because a "would block" error was seen.
    ///
    /// The flag indicating that this stream is readable is unset and the
    /// current task is scheduled to receive a notification when the stream is
    /// then again readable.
    pub fn need_read(&self) {
        self.readiness.fetch_and(!1, Ordering::SeqCst);
        self.handle.schedule_read(&self.token);
    }

    /// Indicates to this source of events that the corresponding I/O object is
    /// no longer writable, but it needs to be.
    ///
    /// This function, like `poll_write`, is only safe to call from the context
    /// of a future's task (typically in a `Future::poll` implementation). It
    /// informs this readiness stream that the underlying object is no longer
    /// writable, typically because a "would block" error was seen.
    ///
    /// The flag indicating that this stream is writable is unset and the
    /// current task is scheduled to receive a notification when the stream is
    /// then again writable.
    pub fn need_write(&self) {
        self.readiness.fetch_and(!2, Ordering::SeqCst);
        self.handle.schedule_write(&self.token);
    }

    /// Returns a reference to the event loop handle that this readiness stream
    /// is associated with.
    pub fn loop_handle(&self) -> &LoopHandle {
        &self.handle
    }

    /// Returns a shared reference to the underlying I/O object this readiness
    /// stream is wrapping.
    pub fn get_ref(&self) -> &E {
        &self.io
    }

    /// Returns a mutable reference to the underlying I/O object this readiness
    /// stream is wrapping.
    pub fn get_mut(&mut self) -> &mut E {
        &mut self.io
    }
}

impl<E> Future for ReadinessStreamNew<E>
    where E: mio::Evented + Send + 'static,
{
    type Item = ReadinessStream<E>;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<ReadinessStream<E>, io::Error> {
        let (io, token) = try_ready!(self.inner.poll());
        Ok(ReadinessStream {
            token: token,
            handle: self.handle.clone(),
            io: io,
            readiness: AtomicUsize::new(0),
        }.into())
    }
}

impl<E> Drop for ReadinessStream<E> {
    fn drop(&mut self) {
        self.handle.drop_source(&self.token)
    }
}
