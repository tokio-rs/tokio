use crate::io::{AsyncRead, AsyncWrite};
use crate::net::driver::{Readiness, Registration};

use futures_core::ready;
use mio;
use std::fmt;
use std::io::{self, Read, Write};
use std::marker::Unpin;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::task::{Context, Poll};

/// Associates an I/O resource that implements the [`std::io::Read`] and/or
/// [`std::io::Write`] traits with the reactor that drives it.
///
/// `IoResource` uses [`Registration`] to take a type that implements
/// [`mio::event::Source`] as well as [`std::io::Read`] and/or
/// [`std::io::Write`] and associates it with a reactor that will drive it.
///
/// Once the [`mio::event::Source`] type is wrapped by `IoResource`, it can
/// be used from within the future's execution model. As such, the
/// `IoResource` type provides [`AsyncRead`] and [`AsyncWrite`]
/// implementations using the underlying I/O resource as well as readiness
/// events provided by the reactor.
///
/// **Note**: While `IoResource` is `Sync` (if the underlying I/O type is
/// `Sync`), the caller must ensure that there are at most two tasks that use
/// a `IoResource` instance concurrently. One for reading and one for writing.
/// While violating this requirement is "safe" from a Rust memory model point
/// of view, it will result in unexpected behavior in the form of lost
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
/// current readiness state of the `IoResource` instance. If
/// [`poll_read_ready`] indicates read readiness, immediately calling
/// [`poll_read_ready`] again will also indicate read readiness.
///
/// When the operation is attempted and is unable to succeed due to the I/O
/// resource not being ready, the caller must call [`clear_read_ready`] or
/// [`clear_write_ready`]. This clears the readiness state until a new
/// readiness event is received.
///
/// This allows the caller to implement additional functions. For example,
/// [`TcpListener`] implements poll_accept by using [`poll_read_ready`] and
/// [`clear_read_ready`].
///
/// TODO
/// ```rust
/// use tokio::net::util::IoResource;
///
/// use futures_core::ready;
/// use tokio::net::driver::Readiness;
/// use mio::net::{TcpStream, TcpListener};
/// use std::io;
/// use std::task::{Context, Poll};
///
/// struct MyListener {
///     poll_evented: IoResource<TcpListener>,
/// }
///
/// impl MyListener {
///     pub fn poll_accept(&mut self, cx: &mut Context<'_>) -> Poll<Result<TcpStream, io::Error>> {
///         let readiness = Readiness::readable();
///
///         ready!(self.poll_evented.poll_read_ready(cx))?;
///
///         match self.poll_evented.get_ref().accept() {
///             Ok((socket, _)) => Poll::Ready(Ok(socket)),
///             Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
///                 self.poll_evented.clear_read_ready(cx)?;
///                 Poll::Pending
///             }
///             Err(e) => Poll::Ready(Err(e)),
///         }
///     }
/// }
/// ```
///
/// TODO
/// ## Platform-specific events?
///
/// [`std::io::Read`]: https://doc.rust-lang.org/std/io/trait.Read.html
/// [`std::io::Write`]: https://doc.rust-lang.org/std/io/trait.Write.html
/// [`AsyncRead`]: ../io/trait.AsyncRead.html
/// [`AsyncWrite`]: ../io/trait.AsyncWrite.html
/// [`mio::Evented`]: https://docs.rs/mio/0.6/mio/trait.Evented.html
/// [`Registration`]: struct.Registration.html
/// [`TcpListener`]: ../net/struct.TcpListener.html
/// [`clear_read_ready`]: #method.clear_read_ready
/// [`clear_write_ready`]: #method.clear_write_ready
/// [`poll_read_ready`]: #method.poll_read_ready
/// [`poll_write_ready`]: #method.poll_write_ready
pub struct IoResource<S: mio::event::Source> {
    io: Option<S>,
    registration: Registration,
    /// Cached read readiness
    read_readiness: AtomicUsize,
    /// Cached write readiness
    write_readiness: AtomicUsize,
}

macro_rules! poll_readiness {
    ($me:expr, $cache:ident, $mask:ident, $take:ident, $poll:expr) => {{
        // Load cached readiness and check if it currently has readiness
        // matching `$mask`.
        let mut cached = $me.$cache.load(Relaxed);
        let mut current = $mask & Readiness::from_usize(cached);

        // If readiness is currently empty, loop until there is readiness;
        // otherwise consume pending events and return the new readiness.
        if current.is_empty() {
            loop {
                let new = match $poll? {
                    Poll::Ready(v) => v,
                    Poll::Pending => return Poll::Pending,
                };
                cached |= new.as_usize();

                // Store the cached readiness.
                $me.$cache.store(cached, Relaxed);

                current |= $mask & new;
                if !current.is_empty() {
                    return Poll::Ready(Ok(current));
                }
            }
        } else {
            if let Some(readiness) = $me.registration.$take()? {
                cached |= readiness.as_usize();
                $me.$cache.store(cached, Relaxed);
            }
            Poll::Ready(Ok(Readiness::from_usize(cached)))
        }
    }};
}

macro_rules! clear_readiness {
    ($me:expr, $cache:ident, $mask:expr, $poll:expr, $waker:expr) => {{
        $me.$cache.fetch_and(!$mask.as_usize(), Relaxed);
        if $poll?.is_ready() {
            // Notify the current task
            $waker.wake_by_ref()
        }
        Ok(())
    }};
}

impl<S> IoResource<S>
where
    S: mio::event::Source,
{
    /// Creates a new `IoResource` associated with the default reactor.
    pub fn new(io: S) -> io::Result<Self> {
        let registration = Registration::new(&io)?;
        Ok(Self {
            io: Some(io),
            registration,
            read_readiness: AtomicUsize::default(),
            write_readiness: AtomicUsize::default(),
        })
    }

    /// Returns a shared reference to the underlying I/O resource this
    /// readiness stream is wrapping.
    pub fn get_ref(&self) -> &S {
        self.io.as_ref().unwrap()
    }

    /// Returns a mutable reference to the underlying I/O resource this
    /// readiness stream is wrapping.
    pub fn get_mut(&mut self) -> &mut S {
        self.io.as_mut().unwrap()
    }

    /// Consumes self, returning the inner I/O resource.
    ///
    /// This function will deregister the I/O resource from the reactor before
    /// returning. If the deregistration operation fails, an error is
    /// returned.
    ///
    /// **Note**: Deregistering does not guarantee that the I/O resource can be
    /// registered with a different reactor. Some I/O resource types can only
    /// be associated with a single reactor instance for their lifetime.
    pub fn into_inner(mut self) -> io::Result<S> {
        let io = self.io.take().unwrap();
        self.registration.deregister(&io)?;
        Ok(io)
    }

    /// Check the I/O resource's read readiness state.
    ///
    /// TODO: CLOSED
    ///
    /// If the resource is not readable then `Poll::Pending` is returned and
    /// the current task is notified once a new event is received.
    ///
    /// The I/O resource will remain in a read-ready state until readiness is
    /// cleared by calling [`clear_read_ready`].
    ///
    /// [`clear_read_ready`]: #method.clear_read_ready
    ///
    /// # Panics
    ///
    /// This function panics if:
    ///
    /// * called from outside of a task context.
    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Readiness>> {
        let mask = Readiness::readable() | Readiness::read_closed();
        poll_readiness!(
            self,
            read_readiness,
            mask,
            take_read_ready,
            self.registration.poll_read_ready(cx)
        )
    }

    /// Clears the I/O resource's read readiness state and registers the
    /// current task to be notified once a read readiness event is received.
    ///
    /// After calling this function, `poll_read_ready` will return
    /// `Poll::Pending` until a new read readiness event has been received.
    ///
    /// # Panics
    ///
    /// This function panics if:
    ///
    /// * called from outside of a task context.
    pub fn clear_read_ready(&self, cx: &mut Context<'_>) -> io::Result<()> {
        clear_readiness!(
            self,
            read_readiness,
            Readiness::readable(),
            self.registration.poll_read_ready(cx),
            cx.waker()
        )
    }

    /// Check the I/O resource's write readiness state.
    ///
    /// TODO: CLOSED
    ///
    /// If the resource is not writable then `Poll::Pending` is returned and
    /// the current task is notified once a new event is received.
    ///
    /// The I/O resource will remain in a write-ready state until readiness is
    /// cleared by calling [`clear_write_ready`].
    ///
    /// [`clear_write_ready`]: #method.clear_write_ready
    ///
    /// # Panics
    ///
    /// This function panics if:
    ///
    /// * called from outside of a task context.
    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Readiness>> {
        let mask = Readiness::writable() | Readiness::write_closed();
        poll_readiness!(
            self,
            write_readiness,
            mask,
            take_write_ready,
            self.registration.poll_write_ready(cx)
        )
    }

    /// Resets the I/O resource's write readiness state and registers the current
    /// task to be notified once a write readiness event is received.
    ///
    ///
    /// After calling this function, `poll_write_ready` will return
    /// `Poll::Pending` until a new write readiness event has been received.
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn clear_write_ready(&self, cx: &mut Context<'_>) -> io::Result<()> {
        clear_readiness!(
            self,
            write_readiness,
            Readiness::writable(),
            self.registration.poll_write_ready(cx),
            cx.waker()
        )
    }
}

// ===== Read / Write impls =====

impl<S> AsyncRead for IoResource<S>
where
    S: mio::event::Source + Read + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.poll_read_ready(cx))?;

        let r = (*self).get_mut().read(buf);

        if is_wouldblock(&r) {
            self.clear_read_ready(cx)?;
            return Poll::Pending;
        }

        Poll::Ready(r)
    }
}

impl<S> AsyncWrite for IoResource<S>
where
    S: mio::event::Source + Write + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.poll_write_ready(cx))?;

        let r = (*self).get_mut().write(buf);

        if is_wouldblock(&r) {
            self.clear_write_ready(cx)?;
            return Poll::Pending;
        }

        Poll::Ready(r)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.poll_write_ready(cx))?;

        let r = (*self).get_mut().flush();

        if is_wouldblock(&r) {
            self.clear_write_ready(cx)?;
            return Poll::Pending;
        }

        Poll::Ready(r)
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn is_wouldblock<T>(r: &io::Result<T>) -> bool {
    match *r {
        Ok(_) => false,
        Err(ref e) => e.kind() == io::ErrorKind::WouldBlock,
    }
}

impl<S: mio::event::Source + fmt::Debug> fmt::Debug for IoResource<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Source").field("io", &self.io).finish()
    }
}

impl<S: mio::event::Source> Drop for IoResource<S> {
    fn drop(&mut self) {
        if let Some(io) = self.io.take() {
            // Ignore errors
            let _ = self.registration.deregister(&io);
        }
    }
}
