//!
//! Readiness tracking streams, backing I/O objects.
//!
//! This module contains the core type which is used to back all I/O on object
//! in `tokio-core`. The `PollEvented` type is the implementation detail of
//! all I/O. Each `PollEvented` manages registration with a reactor,
//! acquisition of a token, and tracking of the readiness state on the
//! underlying I/O primitive.

#![allow(warnings)]

use reactor::Handle;
use reactor::registration::Registration;

use futures::{task, Async, Poll};
use mio;
use mio::event::Evented;
use tokio_io::{AsyncRead, AsyncWrite};

use std::fmt;
use std::io::{self, Read, Write};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;

/// A concrete implementation of a stream of readiness notifications for I/O
/// objects that originates from an event loop.
///
/// Created by the `PollEvented::new` method, each `PollEvented` is
/// associated with a specific event loop and source of events that will be
/// registered with an event loop.
///
/// An instance of `PollEvented` is essentially the bridge between the `mio`
/// world and the `tokio-core` world, providing abstractions to receive
/// notifications about changes to an object's `mio::Ready` state.
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
///
/// You can find more information about creating a custom I/O object [online].
///
/// [online]: https://tokio.rs/docs/going-deeper-tokio/core-low-level/#custom-io
///
/// ## Readiness to read/write
///
/// A `PollEvented` allows listening and waiting for an arbitrary `mio::Ready`
/// instance, including the platform-specific contents of `mio::Ready`. At most
/// two future tasks, however, can be waiting on a `PollEvented`. The
/// `need_read` and `need_write` methods can block two separate tasks, one on
/// reading and one on writing. Not all I/O events correspond to read/write,
/// however!
///
/// To account for this a `PollEvented` gets a little interesting when working
/// with an arbitrary instance of `mio::Ready` that may not map precisely to
/// "write" and "read" tasks. Currently it is defined that instances of
/// `mio::Ready` that do *not* return true from `is_writable` are all notified
/// through `need_read`, or the read task.
///
/// In other words, `poll_ready` with the `mio::UnixReady::hup` event will block
/// the read task of this `PollEvented` if the `hup` event isn't available.
/// Essentially a good rule of thumb is that if you're using the `poll_ready`
/// method you want to also use `need_read` to signal blocking and you should
/// otherwise probably avoid using two tasks on the same `PollEvented`.
pub struct PollEvented<E> {
    io: E,
    inner: Inner,
}

struct Inner {
    registration: Registration,

    /// Currently visible read readiness
    read_readiness: AtomicUsize,

    /// Currently visible write readiness
    write_readiness: AtomicUsize,
}

// ===== impl PollEvented =====

impl<E> PollEvented<E>
where E: Evented
{
    /// Creates a new `PollEvented` associated with the default reactor.
    pub fn new(io: E) -> PollEvented<E> {
        PollEvented {
            io: io,
            inner: Inner {
                registration: Registration::new(),
                read_readiness: AtomicUsize::new(0),
                write_readiness: AtomicUsize::new(0),
            }
        }
    }

    /// Creates a new `PollEvented` associated with the specified reactor.
    pub fn new_with_handle(io: E, handle: &Handle) -> io::Result<Self> {
        let ret = PollEvented::new(io);
        ret.inner.registration.register_with(&ret.io, handle)?;
        Ok(ret)
    }

    /// Tests to see if this source is ready to be read from or not.
    ///
    /// If this stream is not ready for a read then `Async::NotReady` will be
    /// returned and the current task will be scheduled to receive a
    /// notification when the stream is readable again. In other words, this
    /// method is only safe to call from within the context of a future's task,
    /// typically done in a `Future::poll` method.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn poll_read_ready(&self) -> Poll<mio::Ready, io::Error> {
        self.register()?;

        // Load the cached readiness
        match self.inner.read_readiness.load(Relaxed) {
            0 => {}
            mut n => {
                // Check what's new with the reactor.
                if let Some(ready) = self.inner.registration.take_read_ready()? {
                    n |= super::ready2usize(ready);
                    self.inner.read_readiness.store(n, Relaxed);
                }

                return Ok(super::usize2ready(n).into());
            }
        }

        let ready = try_ready!(self.inner.registration.poll_read_ready());

        // Cache the value
        self.inner.read_readiness.store(super::ready2usize(ready), Relaxed);

        Ok(ready.into())
    }

    /// Indicates to this source of events that the corresponding I/O object is
    /// no longer readable, but it needs to be.
    ///
    /// This function, like `poll_read`, is only safe to call from the context
    /// of a future's task (typically in a `Future::poll` implementation). It
    /// informs this readiness stream that the underlying object is no longer
    /// readable, typically because a "would block" error was seen.
    ///
    /// *All* readiness bits associated with this stream except the writable bit
    /// will be reset when this method is called. The current task is then
    /// scheduled to receive a notification whenever anything changes other than
    /// the writable bit. Note that this typically just means the readable bit
    /// is used here, but if you're using a custom I/O object for events like
    /// hup/error this may also be relevant.
    ///
    /// Note that it is also only valid to call this method if `poll_read`
    /// previously indicated that the object is readable. That is, this function
    /// must always be paired with calls to `poll_read` previously.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn need_read(&self) -> io::Result<()> {
        self.inner.read_readiness.store(0, Relaxed);

        if self.poll_read_ready()?.is_ready() {
            // Notify the current task
            task::current().notify();
        }

        Ok(())
    }

    /// Tests to see if this source is ready to be written to or not.
    ///
    /// If this stream is not ready for a write then `Async::NotReady` will be
    /// returned and the current task will be scheduled to receive a
    /// notification when the stream is writable again. In other words, this
    /// method is only safe to call from within the context of a future's task,
    /// typically done in a `Future::poll` method.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn poll_write_ready(&self) -> Poll<mio::Ready, io::Error> {
        self.register()?;

        match self.inner.write_readiness.load(Relaxed) {
            0 => {}
            mut n => {
                // Check what's new with the reactor.
                if let Some(ready) = self.inner.registration.take_write_ready()? {
                    n |= super::ready2usize(ready);
                    self.inner.write_readiness.store(n, Relaxed);
                }

                return Ok(super::usize2ready(n).into());
            }
        }

        let ready = try_ready!(self.inner.registration.poll_write_ready());

        // Cache the value
        self.inner.write_readiness.store(super::ready2usize(ready), Relaxed);

        Ok(ready.into())
    }

    /// Indicates to this source of events that the corresponding I/O object is
    /// no longer writable, but it needs to be.
    ///
    /// This function, like `poll_write_ready`, is only safe to call from the
    /// context of a future's task (typically in a `Future::poll`
    /// implementation). It informs this readiness stream that the underlying
    /// object is no longer writable, typically because a "would block" error
    /// was seen.
    ///
    /// The flag indicating that this stream is writable is unset and the
    /// current task is scheduled to receive a notification when the stream is
    /// then again writable.
    ///
    /// Note that it is also only valid to call this method if
    /// `poll_write_ready` previously indicated that the object is writable.
    /// That is, this function must always be paired with calls to `poll_write`
    /// previously.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn need_write(&self) -> io::Result<()> {
        self.inner.write_readiness.store(0, Relaxed);

        if self.poll_write_ready()?.is_ready() {
            // Notify the current task
            task::current().notify();
        }

        Ok(())
    }

    /// Ensure that the I/O resource is registered with the reactor.
    fn register(&self) -> io::Result<()> {
        self.inner.registration.register(&self.io)?;
        Ok(())
    }
}

impl<E> PollEvented<E> {
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

    /// Consumes self, returning the inner I/O object
    pub fn into_inner(self) -> E {
        self.io
    }
}

// ===== Read / Write impls =====

impl<E> Read for PollEvented<E>
where E: Evented + Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Async::NotReady = self.poll_read_ready()? {
            return Err(io::ErrorKind::WouldBlock.into())
        }

        let r = self.get_mut().read(buf);

        if is_wouldblock(&r) {
            self.need_read()?;
        }

        return r
    }
}

impl<E> Write for PollEvented<E>
where E: Evented + Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Async::NotReady = self.poll_write_ready()? {
            return Err(io::ErrorKind::WouldBlock.into())
        }

        let r = self.get_mut().write(buf);

        if is_wouldblock(&r) {
            self.need_write()?;
        }

        return r
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Async::NotReady = self.poll_write_ready()? {
            return Err(io::ErrorKind::WouldBlock.into())
        }

        let r = self.get_mut().flush();

        if is_wouldblock(&r) {
            self.need_write()?;
        }

        return r
    }
}

impl<E> AsyncRead for PollEvented<E>
where E: Evented + Read,
{
}

impl<E> AsyncWrite for PollEvented<E>
where E: Evented + Write,
{
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        Ok(().into())
    }
}

// ===== &'a Read / &'a Write impls =====

impl<'a, E> Read for &'a PollEvented<E>
where E: Evented, &'a E: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Async::NotReady = self.poll_read_ready()? {
            return Err(io::ErrorKind::WouldBlock.into())
        }

        let r = self.get_ref().read(buf);

        if is_wouldblock(&r) {
            self.need_read()?;
        }

        return r
    }
}

impl<'a, E> Write for &'a PollEvented<E>
where E: Evented, &'a E: Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Async::NotReady = self.poll_write_ready()? {
            return Err(io::ErrorKind::WouldBlock.into())
        }

        let r = self.get_ref().write(buf);

        if is_wouldblock(&r) {
            self.need_write()?;
        }

        return r
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Async::NotReady = self.poll_write_ready()? {
            return Err(io::ErrorKind::WouldBlock.into())
        }

        let r = self.get_ref().flush();

        if is_wouldblock(&r) {
            self.need_write()?;
        }

        return r
    }
}

impl<'a, E> AsyncRead for &'a PollEvented<E>
where E: Evented, &'a E: Read,
{
}

impl<'a, E> AsyncWrite for &'a PollEvented<E>
where E: Evented, &'a E: Write,
{
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        Ok(().into())
    }
}

fn is_wouldblock<T>(r: &io::Result<T>) -> bool {
    match *r {
        Ok(_) => false,
        Err(ref e) => e.kind() == io::ErrorKind::WouldBlock,
    }
}


impl<E: fmt::Debug> fmt::Debug for PollEvented<E> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("PollEvented")
         .field("io", &self.io)
         .finish()
    }
}
