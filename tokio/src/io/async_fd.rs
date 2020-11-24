use crate::io::driver::{Handle, Interest, ReadyEvent, Registration};

use mio::unix::SourceFd;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::{task::Context, task::Poll};

/// Associates an IO object backed by a Unix file descriptor with the tokio
/// reactor, allowing for readiness to be polled. The file descriptor must be of
/// a type that can be used with the OS polling facilities (ie, `poll`, `epoll`,
/// `kqueue`, etc), such as a network socket or pipe.
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
/// The inner object is required to implement [`AsRawFd`]. This file descriptor
/// must not change while [`AsyncFd`] owns the inner object. Changing the file
/// descriptor results in unspecified behavior in the IO driver, which may
/// include breaking notifications for other sockets/etc.
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
/// [`AsyncFdReadyGuard`] returned from readiness checking functions serves this
/// function; after calling a readiness-checking async function, you must use
/// this [`AsyncFdReadyGuard`] to signal to tokio whether the file descriptor is no
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
/// [`AsyncFdReadyGuard`]: struct@self::AsyncFdReadyGuard
/// [`TcpStream::poll_read_ready`]: struct@crate::net::TcpStream
pub struct AsyncFd<T: AsRawFd> {
    registration: Registration,
    inner: Option<T>,
}
/// Represents an IO-ready event detected on a particular file descriptor, which
/// has not yet been acknowledged. This is a `must_use` structure to help ensure
/// that you do not forget to explicitly clear (or not clear) the event.
#[must_use = "You must explicitly choose whether to clear the readiness state by calling a method on ReadyGuard"]
pub struct AsyncFdReadyGuard<'a, T: AsRawFd> {
    async_fd: &'a AsyncFd<T>,
    event: Option<ReadyEvent>,
}

const ALL_INTEREST: Interest = Interest::READABLE.add(Interest::WRITABLE);

impl<T: AsRawFd> AsyncFd<T> {
    #[inline]
    /// Creates an AsyncFd backed by (and taking ownership of) an object
    /// implementing [`AsRawFd`]. The backing file descriptor is cached at the
    /// time of creation.
    ///
    /// This function must be called in the context of a tokio runtime.
    pub fn new(inner: T) -> io::Result<Self>
    where
        T: AsRawFd,
    {
        Self::with_interest(inner, ALL_INTEREST)
    }

    #[inline]
    /// Creates new instance as `new` with additional ability to customize interest,
    /// allowing to specify whether file descriptor will be polled for read, write or both.
    pub fn with_interest(inner: T, interest: Interest) -> io::Result<Self>
    where
        T: AsRawFd,
    {
        Self::new_with_handle_and_interest(inner, Handle::current(), interest)
    }

    pub(crate) fn new_with_handle_and_interest(
        inner: T,
        handle: Handle,
        interest: Interest,
    ) -> io::Result<Self> {
        let fd = inner.as_raw_fd();

        let registration =
            Registration::new_with_interest_and_handle(&mut SourceFd(&fd), interest, handle)?;

        Ok(AsyncFd {
            registration,
            inner: Some(inner),
        })
    }

    /// Returns a shared reference to the backing object of this [`AsyncFd`]
    #[inline]
    pub fn get_ref(&self) -> &T {
        self.inner.as_ref().unwrap()
    }

    /// Returns a mutable reference to the backing object of this [`AsyncFd`]
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.as_mut().unwrap()
    }

    fn take_inner(&mut self) -> Option<T> {
        let fd = self.inner.as_ref().map(AsRawFd::as_raw_fd);

        if let Some(fd) = fd {
            let _ = self.registration.deregister(&mut SourceFd(&fd));
        }

        self.inner.take()
    }

    /// Deregisters this file descriptor, and returns ownership of the backing
    /// object.
    pub fn into_inner(mut self) -> T {
        self.take_inner().unwrap()
    }

    /// Polls for read readiness. This function retains the waker for the last
    /// context that called [`poll_read_ready`]; it therefore can only be used
    /// by a single task at a time (however, [`poll_write_ready`] retains a
    /// second, independent waker).
    ///
    /// This function is intended for cases where creating and pinning a future
    /// via [`readable`] is not feasible. Where possible, using [`readable`] is
    /// preferred, as this supports polling from multiple tasks at once.
    ///
    /// [`poll_read_ready`]: method@Self::poll_read_ready
    /// [`poll_write_ready`]: method@Self::poll_write_ready
    /// [`readable`]: method@Self::readable
    pub fn poll_read_ready<'a>(
        &'a self,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<AsyncFdReadyGuard<'a, T>>> {
        let event = ready!(self.registration.poll_read_ready(cx))?;

        Ok(AsyncFdReadyGuard {
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
    ///
    /// [`poll_read_ready`]: method@Self::poll_read_ready
    /// [`poll_write_ready`]: method@Self::poll_write_ready
    /// [`writable`]: method@Self::writable
    pub fn poll_write_ready<'a>(
        &'a self,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<AsyncFdReadyGuard<'a, T>>> {
        let event = ready!(self.registration.poll_write_ready(cx))?;

        Ok(AsyncFdReadyGuard {
            async_fd: self,
            event: Some(event),
        })
        .into()
    }

    async fn readiness(&self, interest: Interest) -> io::Result<AsyncFdReadyGuard<'_, T>> {
        let event = self.registration.readiness(interest).await?;

        Ok(AsyncFdReadyGuard {
            async_fd: self,
            event: Some(event),
        })
    }

    /// Waits for the file descriptor to become readable, returning a
    /// [`AsyncFdReadyGuard`] that must be dropped to resume read-readiness polling.
    ///
    /// [`AsyncFdReadyGuard`]: struct@self::AsyncFdReadyGuard
    pub async fn readable(&self) -> io::Result<AsyncFdReadyGuard<'_, T>> {
        self.readiness(Interest::READABLE).await
    }

    /// Waits for the file descriptor to become writable, returning a
    /// [`AsyncFdReadyGuard`] that must be dropped to resume write-readiness polling.
    ///
    /// [`AsyncFdReadyGuard`]: struct@self::AsyncFdReadyGuard
    pub async fn writable(&self) -> io::Result<AsyncFdReadyGuard<'_, T>> {
        self.readiness(Interest::WRITABLE).await
    }
}

impl<T: AsRawFd> AsRawFd for AsyncFd<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_ref().unwrap().as_raw_fd()
    }
}

impl<T: std::fmt::Debug + AsRawFd> std::fmt::Debug for AsyncFd<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncFd")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<T: AsRawFd> Drop for AsyncFd<T> {
    fn drop(&mut self) {
        let _ = self.take_inner();
    }
}

impl<'a, Inner: AsRawFd> AsyncFdReadyGuard<'a, Inner> {
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
            self.async_fd.registration.clear_readiness(event);
        }
    }

    /// This function should be invoked when you intentionally want to keep the
    /// ready flag asserted.
    ///
    /// While this function is itself a no-op, it satisfies the `#[must_use]`
    /// constraint on the [`AsyncFdReadyGuard`] type.
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
    /// `AsyncFdReadyGuard` no longer expresses the readiness state that was queried to
    /// create this `AsyncFdReadyGuard`.
    ///
    /// [`WouldBlock`]: std::io::ErrorKind::WouldBlock
    pub fn with_io<R>(&mut self, f: impl FnOnce() -> io::Result<R>) -> io::Result<R> {
        let result = f();

        if let Err(e) = result.as_ref() {
            if e.kind() == io::ErrorKind::WouldBlock {
                self.clear_ready();
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
    /// `AsyncFdReadyGuard` no longer expresses the readiness state that was queried to
    /// create this `AsyncFdReadyGuard`.
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

impl<'a, T: std::fmt::Debug + AsRawFd> std::fmt::Debug for AsyncFdReadyGuard<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadyGuard")
            .field("async_fd", &self.async_fd)
            .finish()
    }
}
