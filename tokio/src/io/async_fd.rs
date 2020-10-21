use std::os::unix::io::{AsRawFd, RawFd};
use std::{task::Context, task::Poll};

use std::io;

use mio::unix::SourceFd;

use crate::io::driver::{Direction, Handle, ReadyEvent, ScheduledIo};
use crate::util::slab;

/// Associates an IO object backed by a Unix file descriptor with the tokio
/// reactor, allowing for readiness to be polled.
///
/// Creating an AsyncFd registers the file descriptor with the current tokio
/// Reactor, allowing you to directly await the file descriptor being readable
/// or writable. Once registered, the file descriptor remains registered until
/// the AsyncFd is dropped.
///
/// The AsyncFd takes ownership of an arbitrary object to represent the IO
/// object. It is intended that this object will handle closing the file
/// descriptor when it is dropped, avoiding resource leaks and ensuring that the
/// AsyncFd can clean up the registration before closing the file descriptor.
/// The [`AsyncFd::into_inner`] function can be used to extract the inner object
/// to retake control from the tokio IO reactor.
///
/// Polling for readiness is done by calling the async functions [`readable`]
/// and [`writable`]. These functions complete when the associated readiness
/// condition is observed. Any number of tasks can query the same `AsyncFd` in
/// parallel, on the same or different conditions.
///
/// On some platforms, the readiness detecting mechanism relies on
/// edge-triggered notifications. This means that the OS will only notify Tokio
/// when the file descriptor transitions from not-ready to ready. Tokio
/// internally tracks when it has received a ready notification, and when
/// readiness checking functions like [`readable`] and [`writable`] are called,
/// if the readiness flag is set, these async functions will complete
/// immediately.
///
/// This however does mean that it is critical to ensure that this ready flag is
/// cleared when (and only when) the file descriptor ceases to be ready. The
/// [`ReadyGuard`] returned from readiness checking functions serves this
/// function; after calling a readiness-checking async function, you must use
/// this [`ReadyGuard`] to signal to tokio whether the file descriptor is no
/// longer in a ready state.
///
/// ## Use with to a poll-based API
///
/// In some cases it may be desirable to use `AsyncFd` from APIs similar to
/// [`TcpStream::poll_read_ready`]. The [`AsyncFd::poll_read_ready`] and
/// [`AsyncFd::poll_write_ready`] functions are provided for this purpose.
/// Because these functions don't create a future to hold their state, they have
/// the limitation that only one task can wait on each direction (read or write)
/// at a time.
///
/// [`readable`]: method@Self::readable
/// [`writable`]: method@Self::writable
/// [`ReadyGuard`]: struct@self::ReadyGuard
/// [`TcpStream::poll_read_ready`]: struct@crate::net::TcpStream
pub struct AsyncFd<T> {
    handle: Handle,
    fd: RawFd,
    shared: slab::Ref<ScheduledIo>,
    inner: Option<T>,
}

impl<T> AsRawFd for AsyncFd<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for AsyncFd<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncFd")
            .field("fd", &self.fd)
            .field("inner", &self.inner)
            .finish()
    }
}

const ALL_INTEREST: mio::Interest = mio::Interest::READABLE.add(mio::Interest::WRITABLE);

/// Represents an IO-ready event detected on a particular file descriptor, which
/// has not yet been acknowledged. This is a `must_use` structure to help ensure
/// that you do not forget to explicitly clear (or not clear) the event.
#[must_use = "You must explicitly choose whether to clear the readiness state by calling a method on ReadyGuard"]
pub struct ReadyGuard<'a, T> {
    async_fd: &'a AsyncFd<T>,
    event: Option<ReadyEvent>,
}

impl<'a, T: std::fmt::Debug> std::fmt::Debug for ReadyGuard<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadyGuard")
            .field("async_fd", self.async_fd)
            .finish()
    }
}

impl<'a, T> ReadyGuard<'a, T> {
    /// Indicates to tokio that the file descriptor is no longer ready. The
    /// internal readiness flag will be cleared, and tokio will wait for the
    /// next edge-triggered readiness notification from the OS.
    ///
    /// It is critical that this function not be called unless your code
    /// _actually observes_ that the file descriptor is _not_ ready. Do not call
    /// it simply because, for example, a read succeeded; it should be called
    /// when a read is observed to block.
    ///
    /// [`drop`]: method@std::mem::drop
    pub fn clear_ready(&mut self) {
        if let Some(event) = self.event.take() {
            self.async_fd.shared.clear_readiness(event);
        }
    }

    /// This function should be invoked when you intentionally want to keep the
    /// ready flag asserted.
    ///
    /// While this function is itself a no-op, it satisfies the `#[must_use]`
    /// constraint on the [`ReadyGuard`] type.
    pub fn retain_ready(&mut self) {
        // no-op
    }

    /// Performs the IO operation `f`; if `f` returns a [`WouldBlock`] error,
    /// the readiness state associated with this file descriptor is cleared.
    ///
    /// This method helps ensure that the readiness state of the underlying file
    /// descriptor remains in sync with the tokio-side readiness state, by
    /// clearing the tokio-side state only when a [`WouldBlock`] condition
    /// occurs. It is the responsibility of the caller to ensure that `f`
    /// returns [`WouldBlock`] only if the file descriptor that originated this
    /// `ReadyGuard` no longer expresses the readiness state that was queried to
    /// create this `ReadyGuard`.
    ///
    /// [`WouldBlock`]: std::io::ErrorKind::WouldBlock
    pub fn with_io<R, E>(&mut self, f: impl FnOnce() -> Result<R, E>) -> Result<R, E>
    where
        E: std::error::Error + 'static,
    {
        use std::error::Error;

        let result = f();

        if let Err(e) = result.as_ref() {
            // Is this a WouldBlock error?
            let mut error_ref: Option<&(dyn Error + 'static)> = Some(e);

            while let Some(current) = error_ref {
                if let Some(e) = Error::downcast_ref::<std::io::Error>(current) {
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        self.clear_ready();
                        break;
                    }
                }
                error_ref = current.source();
            }
        }

        result
    }

    /// Performs the IO operation `f`; if `f` returns [`Pending`], the readiness
    /// state associated with this file descriptor is cleared.
    ///
    /// This method helps ensure that the readiness state of the underlying file
    /// descriptor remains in sync with the tokio-side readiness state, by
    /// clearing the tokio-side state only when a [`Pending`] condition occurs.
    /// It is the responsibility of the caller to ensure that `f` returns
    /// [`Pending`] only if the file descriptor that originated this
    /// `ReadyGuard` no longer expresses the readiness state that was queried to
    /// create this `ReadyGuard`.
    ///
    /// [`Pending`]: std::task::Poll::Pending
    pub fn with_poll<R>(&mut self, f: impl FnOnce() -> std::task::Poll<R>) -> std::task::Poll<R> {
        let result = f();

        if result.is_pending() {
            self.clear_ready();
        }

        result
    }
}

impl<T> Drop for AsyncFd<T> {
    fn drop(&mut self) {
        if let Some(inner) = self.handle.inner() {
            let _ = inner.deregister_source(&mut SourceFd(&self.fd));
        }
    }
}

impl<T> AsyncFd<T> {
    /// Creates an AsyncFd backed by (and taking ownership of) an object
    /// implementing [`AsRawFd`]. The backing file descriptor is cached at the
    /// time of creation.
    ///
    /// This function must be called in the context of a tokio runtime.
    pub fn new(inner: T) -> io::Result<Self>
    where
        T: AsRawFd,
    {
        let fd = inner.as_raw_fd();
        Self::new_with_fd(inner, fd)
    }

    /// Constructs a new AsyncFd, explicitly specifying the backing file
    /// descriptor.
    ///
    /// This function must be called in the context of a tokio runtime.
    pub fn new_with_fd(inner: T, fd: RawFd) -> io::Result<Self> {
        Self::new_with_handle(inner, fd, Handle::current())
    }

    pub(crate) fn new_with_handle(inner: T, fd: RawFd, handle: Handle) -> io::Result<Self> {
        let shared = if let Some(inner) = handle.inner() {
            inner.add_source(&mut SourceFd(&fd), ALL_INTEREST)?
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to find event loop",
            ));
        };

        Ok(AsyncFd {
            handle,
            fd,
            shared,
            inner: Some(inner),
        })
    }

    /// Returns a shared reference to the backing object of this [`AsyncFd`]
    #[inline]
    pub fn inner(&self) -> &T {
        self.inner.as_ref().unwrap()
    }

    /// Returns a mutable reference to the backing object of this [`AsyncFd`]
    #[inline]
    pub fn inner_mut(&mut self) -> &mut T {
        self.inner.as_mut().unwrap()
    }

    /// Deregisters this file descriptor, and returns ownership of the backing
    /// object.
    pub fn into_inner(mut self) -> T {
        self.inner.take().unwrap()
    }

    /// Polls for read readiness. This function retains the waker for the last
    /// context that called [`poll_read_ready`]; it therefore can only be used
    /// by a single task at a time (however, [`poll_write_ready`] retains a
    /// second, independent waker).
    ///
    /// This function is intended for cases where creating and pinning a future
    /// via [`readable`] is not feasible. Where possible, using [`readable`] is
    /// preferred, as this supports polling from multiple tasks at once.
    pub fn poll_read_ready<'a, 'cx>(
        &'a self,
        cx: &mut Context<'cx>,
    ) -> Poll<io::Result<ReadyGuard<'a, T>>> {
        let event = ready!(self.shared.poll_readiness(cx, Direction::Read));

        if !self.handle.is_alive() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "IO driver has terminated",
            ))
            .into();
        }

        Ok(ReadyGuard {
            async_fd: self,
            event: Some(event),
        })
        .into()
    }

    /// Polls for write readiness. This function retains the waker for the last
    /// context that called [`poll_write_ready`]; it therefore can only be used
    /// by a single task at a time (however, [`poll_read_ready`] retains a
    /// second, independent waker).
    ///
    /// This function is intended for cases where creating and pinning a future
    /// via [`writable`] is not feasible. Where possible, using [`writable`] is
    /// preferred, as this supports polling from multiple tasks at once.
    pub fn poll_write_ready<'a, 'cx>(
        &'a self,
        cx: &mut Context<'cx>,
    ) -> Poll<io::Result<ReadyGuard<'a, T>>> {
        let event = ready!(self.shared.poll_readiness(cx, Direction::Write));

        if !self.handle.is_alive() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "IO driver has terminated",
            ))
            .into();
        }

        Ok(ReadyGuard {
            async_fd: self,
            event: Some(event),
        })
        .into()
    }

    async fn readiness(&self, interest: mio::Interest) -> io::Result<ReadyGuard<'_, T>> {
        let event = self.shared.readiness(interest);

        if !self.handle.is_alive() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "IO driver has terminated",
            ));
        }

        let event = event.await;
        Ok(ReadyGuard {
            async_fd: self,
            event: Some(event),
        })
    }

    /// Waits for the file descriptor to become readable, returning a
    /// [`ReadyGuard`] that must be dropped to resume read-readiness polling.
    ///
    /// [`ReadyGuard`]: struct@self::ReadyGuard
    pub async fn readable(&self) -> io::Result<ReadyGuard<'_, T>> {
        self.readiness(mio::Interest::READABLE).await
    }

    /// Waits for the file descriptor to become writable, returning a
    /// [`ReadyGuard`] that must be dropped to resume write-readiness polling.
    ///
    /// [`ReadyGuard`]: struct@self::ReadyGuard
    pub async fn writable(&self) -> io::Result<ReadyGuard<'_, T>> {
        self.readiness(mio::Interest::WRITABLE).await
    }
}
