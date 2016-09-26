//! Readiness tracking streams, backing I/O objects.
//!
//! This module contains the core type which is used to back all I/O on object
//! in `tokio-core`. The `PollEvented` type is the implementation detail of
//! all I/O. Each `PollEvented` manages registration with a reactor,
//! acquisition of a token, and tracking of the readiness state on the
//! underlying I/O primitive.

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::Async;
use mio;

use io::Io;
use reactor::{Handle, Remote};
use reactor::io_token::IoToken;

/// A concrete implementation of a stream of readiness notifications for I/O
/// objects that originates from an event loop.
///
/// Created by the `PollEvented::new` method, each `PollEvented` is
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
pub struct PollEvented<E> {
    token: IoToken,
    handle: Remote,
    readiness: AtomicUsize,
    io: E,
}

impl<E: mio::Evented> PollEvented<E> {
    /// Creates a new readiness stream associated with the provided
    /// `loop_handle` and for the given `source`.
    ///
    /// This method returns a future which will resolve to the readiness stream
    /// when it's ready.
    pub fn new(io: E, handle: &Handle) -> io::Result<PollEvented<E>> {
        Ok(PollEvented {
            token: try!(IoToken::new(&io, handle)),
            handle: handle.remote().clone(),
            readiness: AtomicUsize::new(0),
            io: io,
        })
    }
}

impl<E> PollEvented<E> {
    /// Tests to see if this source is ready to be read from or not.
    ///
    /// If this stream is not ready for a read then `NotReady` will be returned
    /// and the current task will be scheduled to receive a notification when
    /// the stream is readable again. In other words, this method is only safe
    /// to call from within the context of a future's task, typically done in a
    /// `Future::poll` method.
    pub fn poll_read(&self) -> Async<()> {
        if self.readiness.load(Ordering::SeqCst) & 1 != 0 {
            return Async::Ready(())
        }
        self.readiness.fetch_or(self.token.take_readiness(), Ordering::SeqCst);
        if self.readiness.load(Ordering::SeqCst) & 1 != 0 {
            Async::Ready(())
        } else {
            self.token.schedule_read(&self.handle);
            Async::NotReady
        }
    }

    /// Tests to see if this source is ready to be written to or not.
    ///
    /// If this stream is not ready for a write then `NotReady` will be returned
    /// and the current task will be scheduled to receive a notification when
    /// the stream is writable again. In other words, this method is only safe
    /// to call from within the context of a future's task, typically done in a
    /// `Future::poll` method.
    pub fn poll_write(&self) -> Async<()> {
        if self.readiness.load(Ordering::SeqCst) & 2 != 0 {
            return Async::Ready(())
        }
        self.readiness.fetch_or(self.token.take_readiness(), Ordering::SeqCst);
        if self.readiness.load(Ordering::SeqCst) & 2 != 0 {
            Async::Ready(())
        } else {
            self.token.schedule_write(&self.handle);
            Async::NotReady
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
    ///
    /// Note that it is also only valid to call this method if `poll_read`
    /// previously indicated that the object is readable. That is, this function
    /// must always be paired with calls to `poll_read` previously.
    pub fn need_read(&self) {
        self.readiness.fetch_and(!1, Ordering::SeqCst);
        self.token.schedule_read(&self.handle)
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
    ///
    /// Note that it is also only valid to call this method if `poll_write`
    /// previously indicated that the object is writeable. That is, this function
    /// must always be paired with calls to `poll_write` previously.
    pub fn need_write(&self) {
        self.readiness.fetch_and(!2, Ordering::SeqCst);
        self.token.schedule_write(&self.handle)
    }

    /// Returns a reference to the event loop handle that this readiness stream
    /// is associated with.
    pub fn remote(&self) -> &Remote {
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

impl<E: Read> Read for PollEvented<E> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Async::NotReady = self.poll_read() {
            return Err(mio::would_block())
        }
        let r = self.get_mut().read(buf);
        if is_wouldblock(&r) {
            self.need_read();
        }
        return r
    }
}

impl<E: Write> Write for PollEvented<E> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Async::NotReady = self.poll_write() {
            return Err(mio::would_block())
        }
        let r = self.get_mut().write(buf);
        if is_wouldblock(&r) {
            self.need_write();
        }
        return r
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Async::NotReady = self.poll_write() {
            return Err(mio::would_block())
        }
        let r = self.get_mut().flush();
        if is_wouldblock(&r) {
            self.need_write();
        }
        return r
    }
}

impl<E: Read + Write> Io for PollEvented<E> {
    fn poll_read(&mut self) -> Async<()> {
        <PollEvented<E>>::poll_read(self)
    }

    fn poll_write(&mut self) -> Async<()> {
        <PollEvented<E>>::poll_write(self)
    }
}

impl<'a, E> Read for &'a PollEvented<E>
    where &'a E: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Async::NotReady = self.poll_read() {
            return Err(mio::would_block())
        }
        let r = self.get_ref().read(buf);
        if is_wouldblock(&r) {
            self.need_read();
        }
        return r
    }
}

impl<'a, E> Write for &'a PollEvented<E>
    where &'a E: Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Async::NotReady = self.poll_write() {
            return Err(mio::would_block())
        }
        let r = self.get_ref().write(buf);
        if is_wouldblock(&r) {
            self.need_write();
        }
        return r
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Async::NotReady = self.poll_write() {
            return Err(mio::would_block())
        }
        let r = self.get_ref().flush();
        if is_wouldblock(&r) {
            self.need_write();
        }
        return r
    }
}

impl<'a, E> Io for &'a PollEvented<E>
    where &'a E: Read + Write,
{
    fn poll_read(&mut self) -> Async<()> {
        <PollEvented<E>>::poll_read(self)
    }

    fn poll_write(&mut self) -> Async<()> {
        <PollEvented<E>>::poll_write(self)
    }
}

fn is_wouldblock<T>(r: &io::Result<T>) -> bool {
    match *r {
        Ok(_) => false,
        Err(ref e) => e.kind() == io::ErrorKind::WouldBlock,
    }
}

impl<E> Drop for PollEvented<E> {
    fn drop(&mut self) {
        self.token.drop_source(&self.handle);
    }
}
