//! Readiness tracking streams, backing I/O objects.
//!
//! This module contains the core type which is used to back all I/O on object
//! in `tokio-core`. The `PollEvented` type is the implementation detail of
//! all I/O. Each `PollEvented` manages registration with a reactor,
//! acquisition of a token, and tracking of the readiness state on the
//! underlying I/O primitive.

#![allow(deprecated, warnings)]

use std::fmt;
use std::io::{self, Read, Write};
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;

use futures::{task, Async, Poll};
use mio::event::Evented;
use mio::Ready;
use tokio_io::{AsyncRead, AsyncWrite};

use reactor::{Handle, Registration};

#[deprecated(since = "0.1.2", note = "PollEvented2 instead")]
#[doc(hidden)]
pub struct PollEvented<E> {
    io: E,
    inner: Inner,
    handle: Handle,
}

struct Inner {
    registration: Mutex<Registration>,

    /// Currently visible read readiness
    read_readiness: AtomicUsize,

    /// Currently visible write readiness
    write_readiness: AtomicUsize,
}

impl<E: fmt::Debug> fmt::Debug for PollEvented<E> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("PollEvented")
         .field("io", &self.io)
         .finish()
    }
}

impl<E> PollEvented<E> {
    /// Creates a new readiness stream associated with the provided
    /// `loop_handle` and for the given `source`.
    pub fn new(io: E, handle: &Handle) -> io::Result<PollEvented<E>>
        where E: Evented,
    {
        let registration = Registration::new();
        registration.register(&io)?;

        Ok(PollEvented {
            io: io,
            inner: Inner {
                registration: Mutex::new(registration),
                read_readiness: AtomicUsize::new(0),
                write_readiness: AtomicUsize::new(0),
            },
            handle: handle.clone(),
        })
    }

    /// Tests to see if this source is ready to be read from or not.
    ///
    /// If this stream is not ready for a read then `Async::NotReady` will be
    /// returned and the current task will be scheduled to receive a
    /// notification when the stream is readable again. In other words, this
    /// method is only safe to call from within the context of a future's task,
    /// typically done in a `Future::poll` method.
    ///
    /// This is mostly equivalent to `self.poll_ready(Ready::readable())`.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn poll_read(&mut self) -> Async<()> {
        if self.poll_read2().is_ready() {
            return ().into();
        }

        Async::NotReady
    }

    fn poll_read2(&self) -> Async<Ready> {
        let r = self.inner.registration.lock().unwrap();

        // Load the cached readiness
        match self.inner.read_readiness.load(Relaxed) {
            0 => {}
            mut n => {
                // Check what's new with the reactor.
                if let Some(ready) = r.take_read_ready().unwrap() {
                    n |= ready2usize(ready);
                    self.inner.read_readiness.store(n, Relaxed);
                }

                return usize2ready(n).into();
            }
        }

        let ready = match r.poll_read_ready().unwrap() {
            Async::Ready(r) => r,
            _ => return Async::NotReady,
        };

        // Cache the value
        self.inner.read_readiness.store(ready2usize(ready), Relaxed);

        ready.into()
    }

    /// Tests to see if this source is ready to be written to or not.
    ///
    /// If this stream is not ready for a write then `Async::NotReady` will be returned
    /// and the current task will be scheduled to receive a notification when
    /// the stream is writable again. In other words, this method is only safe
    /// to call from within the context of a future's task, typically done in a
    /// `Future::poll` method.
    ///
    /// This is mostly equivalent to `self.poll_ready(Ready::writable())`.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn poll_write(&mut self) -> Async<()> {
        let r = self.inner.registration.lock().unwrap();

        match self.inner.write_readiness.load(Relaxed) {
            0 => {}
            mut n => {
                // Check what's new with the reactor.
                if let Some(ready) = r.take_write_ready().unwrap() {
                    n |= ready2usize(ready);
                    self.inner.write_readiness.store(n, Relaxed);
                }

                return ().into();
            }
        }

        let ready = match r.poll_write_ready().unwrap() {
            Async::Ready(r) => r,
            _ => return Async::NotReady,
        };

        // Cache the value
        self.inner.write_readiness.store(ready2usize(ready), Relaxed);

        ().into()
    }

    /// Test to see whether this source fulfills any condition listed in `mask`
    /// provided.
    ///
    /// The `mask` given here is a mio `Ready` set of possible events. This can
    /// contain any events like read/write but also platform-specific events
    /// such as hup and error. The `mask` indicates events that are interested
    /// in being ready.
    ///
    /// If any event in `mask` is ready then it is returned through
    /// `Async::Ready`. The `Ready` set returned is guaranteed to not be empty
    /// and contains all events that are currently ready in the `mask` provided.
    ///
    /// If no events are ready in the `mask` provided then the current task is
    /// scheduled to receive a notification when any of them become ready. If
    /// the `writable` event is contained within `mask` then this
    /// `PollEvented`'s `write` task will be blocked and otherwise the `read`
    /// task will be blocked. This is generally only relevant if you're working
    /// with this `PollEvented` object on multiple tasks.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn poll_ready(&mut self, mask: Ready) -> Async<Ready> {
        let mut ret = Ready::empty();

        if mask.is_empty() {
            return ret.into();
        }

        if mask.is_writable() {
            if self.poll_write().is_ready() {
                ret = Ready::writable();
            }
        }

        let mask = mask - Ready::writable();

        if !mask.is_empty() {
            if let Async::Ready(v) = self.poll_read2() {
                ret |= v & mask;
            }
        }

        if ret.is_empty() {
            if mask.is_writable() {
                let _ = self.need_write();
            }

            if mask.is_readable() {
                let _ = self.need_read();
            }

            Async::NotReady
        } else {
            ret.into()
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
    /// # Errors
    ///
    /// This function will return an error if the `Reactor` that this `PollEvented`
    /// is associated with has gone away (been destroyed). The error means that
    /// the ambient futures task could not be scheduled to receive a
    /// notification and typically means that the error should be propagated
    /// outwards.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn need_read(&mut self) -> io::Result<()> {
        self.inner.read_readiness.store(0, Relaxed);

        if self.poll_read().is_ready() {
            // Notify the current task
            task::current().notify();
        }

        Ok(())
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
    /// previously indicated that the object is writable. That is, this function
    /// must always be paired with calls to `poll_write` previously.
    ///
    /// # Errors
    ///
    /// This function will return an error if the `Reactor` that this `PollEvented`
    /// is associated with has gone away (been destroyed). The error means that
    /// the ambient futures task could not be scheduled to receive a
    /// notification and typically means that the error should be propagated
    /// outwards.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn need_write(&mut self) -> io::Result<()> {
        self.inner.write_readiness.store(0, Relaxed);

        if self.poll_write().is_ready() {
            // Notify the current task
            task::current().notify();
        }

        Ok(())
    }

    /// Returns a reference to the event loop handle that this readiness stream
    /// is associated with.
    pub fn handle(&self) -> &Handle {
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

    /// Consumes the `PollEvented` and returns the underlying I/O object
    pub fn into_inner(self) -> E {
        self.io
    }

    /// Deregisters this source of events from the reactor core specified.
    ///
    /// This method can optionally be called to unregister the underlying I/O
    /// object with the event loop that the `handle` provided points to.
    /// Typically this method is not required as this automatically happens when
    /// `E` is dropped, but for some use cases the `E` object doesn't represent
    /// an owned reference, so dropping it won't automatically unregister with
    /// the event loop.
    ///
    /// This consumes `self` as it will no longer provide events after the
    /// method is called, and will likely return an error if this `PollEvented`
    /// was created on a separate event loop from the `handle` specified.
    pub fn deregister(&self) -> io::Result<()>
        where E: Evented,
    {
        self.inner.registration.lock().unwrap()
            .deregister(&self.io)
    }
}

impl<E: Read> Read for PollEvented<E> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Async::NotReady = self.poll_read() {
            return Err(io::ErrorKind::WouldBlock.into())
        }

        let r = self.get_mut().read(buf);

        if is_wouldblock(&r) {
            self.need_read()?;
        }

        return r
    }
}

impl<E: Write> Write for PollEvented<E> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Async::NotReady = self.poll_write() {
            return Err(io::ErrorKind::WouldBlock.into())
        }

        let r = self.get_mut().write(buf);

        if is_wouldblock(&r) {
            self.need_write()?;
        }

        return r
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Async::NotReady = self.poll_write() {
            return Err(io::ErrorKind::WouldBlock.into())
        }

        let r = self.get_mut().flush();

        if is_wouldblock(&r) {
            self.need_write()?;
        }

        return r
    }
}

impl<E: Read> AsyncRead for PollEvented<E> {
}

impl<E: Write> AsyncWrite for PollEvented<E> {
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

const READ: usize = 1 << 0;
const WRITE: usize = 1 << 1;

fn ready2usize(ready: Ready) -> usize {
    let mut bits = 0;
    if ready.is_readable() {
        bits |= READ;
    }
    if ready.is_writable() {
        bits |= WRITE;
    }
    bits | platform::ready2usize(ready)
}

fn usize2ready(bits: usize) -> Ready {
    let mut ready = Ready::empty();
    if bits & READ != 0 {
        ready.insert(Ready::readable());
    }
    if bits & WRITE != 0 {
        ready.insert(Ready::writable());
    }
    ready | platform::usize2ready(bits)
}

#[cfg(unix)]
mod platform {
    use mio::Ready;
    use mio::unix::UnixReady;

    const HUP: usize = 1 << 2;
    const ERROR: usize = 1 << 3;
    const AIO: usize = 1 << 4;
    const LIO: usize = 1 << 5;

    #[cfg(any(target_os = "dragonfly", target_os = "freebsd"))]
    fn is_aio(ready: &Ready) -> bool {
        UnixReady::from(*ready).is_aio()
    }

    #[cfg(not(any(target_os = "dragonfly", target_os = "freebsd")))]
    fn is_aio(_ready: &Ready) -> bool {
        false
    }

    #[cfg(target_os = "freebsd")]
    fn is_lio(ready: &Ready) -> bool {
        UnixReady::from(*ready).is_lio()
    }

    #[cfg(not(target_os = "freebsd"))]
    fn is_lio(_ready: &Ready) -> bool {
        false
    }

    pub fn ready2usize(ready: Ready) -> usize {
        let ready = UnixReady::from(ready);
        let mut bits = 0;
        if is_aio(&ready) {
            bits |= AIO;
        }
        if is_lio(&ready) {
            bits |= LIO;
        }
        if ready.is_error() {
            bits |= ERROR;
        }
        if ready.is_hup() {
            bits |= HUP;
        }
        bits
    }

    #[cfg(any(target_os = "dragonfly", target_os = "freebsd", target_os = "ios",
              target_os = "macos"))]
    fn usize2ready_aio(ready: &mut UnixReady) {
        ready.insert(UnixReady::aio());
    }

    #[cfg(not(any(target_os = "dragonfly",
        target_os = "freebsd", target_os = "ios", target_os = "macos")))]
    fn usize2ready_aio(_ready: &mut UnixReady) {
        // aio not available here → empty
    }

    #[cfg(target_os = "freebsd")]
    fn usize2ready_lio(ready: &mut UnixReady) {
        ready.insert(UnixReady::lio());
    }

    #[cfg(not(target_os = "freebsd"))]
    fn usize2ready_lio(_ready: &mut UnixReady) {
        // lio not available here → empty
    }

    pub fn usize2ready(bits: usize) -> Ready {
        let mut ready = UnixReady::from(Ready::empty());
        if bits & AIO != 0 {
            usize2ready_aio(&mut ready);
        }
        if bits & LIO != 0 {
            usize2ready_lio(&mut ready);
        }
        if bits & HUP != 0 {
            ready.insert(UnixReady::hup());
        }
        if bits & ERROR != 0 {
            ready.insert(UnixReady::error());
        }
        ready.into()
    }
}

#[cfg(windows)]
mod platform {
    use mio::Ready;

    pub fn all() -> Ready {
        // No platform-specific Readinesses for Windows
        Ready::empty()
    }

    pub fn hup() -> Ready {
        Ready::empty()
    }

    pub fn ready2usize(_r: Ready) -> usize {
        0
    }

    pub fn usize2ready(_r: usize) -> Ready {
        Ready::empty()
    }
}
