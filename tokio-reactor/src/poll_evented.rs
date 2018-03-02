use {Handle, Registration};

use futures::{task, Async, Poll};
use mio;
use mio::event::Evented;
use tokio_io::{AsyncRead, AsyncWrite};

use std::fmt;
use std::io::{self, Read, Write};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;

/// Associates an I/O resource that implements the [`std::Read`] and / or
/// [`std::Write`] traits with the reactor that drives it.
///
/// `PollEvented` uses [`Registration`] internally to take a type that
/// implements [`mio::Evented`] as well as [`std::Read`] and or [`std::Write`]
/// and associate it with a reactor that will drive it.
///
/// Once the [`mio::Evented`] type is wrapped by `PollEvented`, it can be
/// used from within the future's execution model. As such, the `PollEvented`
/// type provides [`AsyncRead`] and [`AsyncWrite`] implementations using the
/// underlying I/O resource as well as readiness events provided by the reactor.
///
/// **Note**: While `PollEvented` is `Sync` (if the underlying I/O type is
/// `Sync`), the caller must ensure that there are at most two tasks that use a
/// `PollEvented` instance concurrenty. One for reading and one for writing.
/// While violating this requirement is "safe" from a Rust memory model point of
/// view, it will result in unexpected behavior in the form of lost
/// notifications and tasks hanging.
///
/// ## Readiness events
///
/// Besides just providing [`AsyncRead`] and [`AsyncWrite`] implementations,
/// this type also supports access to the underlying readiness event stream.
/// While similar in function to what [`Registration`] provides, the semantics
/// are a bit different.
///
/// Two functions are provided to access the readiness events:
/// [`poll_read_ready`] and [`poll_write_ready`]. These functions return the
/// current readiness state of the `PollEvented` instance. If
/// [`poll_read_ready`] indicates read readiness, immediately calling
/// [`poll_read_ready`] again will also indicate read readiness.
///
/// When the operation is attempted and is unable to succeed due to the I/O
/// resource not being ready, the caller must call [`need_read`] or
/// [`need_write`]. This clears the readiness state until a new readiness event
/// is received.
///
/// This allows the caller to implement additional funcitons. For example,
/// [`TcpListener`] implements accept by using [`poll_read_ready`] and
/// [`need_read`].
///
/// ```rust,ignore
/// pub fn accept(&mut self) -> io::Result<(net::TcpStream, SocketAddr)> {
///     if let Async::NotReady = self.poll_evented.poll_read_ready()? {
///         return Err(io::ErrorKind::WouldBlock.into())
///     }
///
///     match self.poll_evented.get_ref().accept_std() {
///         Ok(pair) => Ok(pair),
///         Err(e) => {
///             if e.kind() == io::ErrorKind::WouldBlock {
///                 self.poll_evented.need_read()?;
///             }
///             Err(e)
///         }
///     }
/// }
/// ```
///
/// ## Platform-specific events
///
/// `PollEvented` also allows receiving platform-specific `mio::Ready` events.
/// These events are included as part of the read readiness event stream. The
/// write readiness event stream is only for `Ready::writable()` events.
///
/// [`std::Read`]: https://doc.rust-lang.org/std/io/trait.Read.html
/// [`std::Write`]: https://doc.rust-lang.org/std/io/trait.Write.html
/// [`AsyncRead`]: ../io/trait.AsyncRead.html
/// [`AsyncWrite`]: ../io/trait.AsyncWrite.html
/// [`mio::Evented`]: https://docs.rs/mio/0.6/mio/trait.Evented.html
/// [`Registration`]: struct.Registration.html
/// [`TcpListener`]: ../net/struct.TcpListener.html
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

    /// Check the I/O resource's read readiness state.
    ///
    /// If the resource is not ready for a read then `Async::NotReady` is
    /// returned and the current task is notified once a new event is received.
    ///
    /// The I/O resource will remain in a read-ready state until readiness is
    /// cleared by calling [`need_read`].
    ///
    /// [`need_read`]: #method.need_read
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
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

    /// Resets the I/O resource's read readiness state and registers the current
    /// task to be notified once a read readiness event is received.
    ///
    /// After calling this function, `poll_read_ready` will return `NotReady`
    /// until a new read readiness event has been received.
    ///
    /// This function clears **all** readiness state **except** write readiness.
    /// This includes any platform-specific readiness bits.
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn need_read(&self) -> io::Result<()> {
        self.inner.read_readiness.store(0, Relaxed);

        if self.poll_read_ready()?.is_ready() {
            // Notify the current task
            task::current().notify();
        }

        Ok(())
    }

    /// Check the I/O resource's write readiness state.
    ///
    /// If the resource is not ready for a write then `Async::NotReady` is
    /// returned and the current task is notified once a new event is received.
    ///
    /// The I/O resource will remain in a write-ready state until readiness is
    /// cleared by calling [`need_write`].
    ///
    /// [`need_write`]: #method.need_write
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
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

    /// Resets the I/O resource's write readiness state and registers the current
    /// task to be notified once a write readiness event is received.
    ///
    /// After calling this function, `poll_write_ready` will return `NotReady`
    /// until a new read readiness event has been received.
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
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
