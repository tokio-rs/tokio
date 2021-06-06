use crate::io::driver::{Handle, Interest, ReadyEvent, Registration};
use mio::event::Source;
use std::fmt;
use std::io;
use std::ops::{Deref, DerefMut};
use std::task::{Context, Poll};

/// Associates a POSIX AIO control block with the reactor that drives it.
///
/// `PollAio`'s wrapped type must implement [`mio::event::Source`] to be driven
/// by the reactor.
///
/// The wrapped source may be accessed through the `PollAio` via the `Deref` and
/// `DerefMut` traits.
///
/// ## Clearing readiness
///
/// If [`PollAio::poll`] returns ready, but the consumer determines that the
/// Source is not completely ready and must return to the Pending state,
/// [`PollAio::clear_ready`] may be used.  This can be useful with
/// [`lio_listio`], which may generate a kevent when only a portion of the
/// operations have completed.
///
/// ## Platforms
///
/// Only FreeBSD implements POSIX AIO with kqueue notification, so
/// `PollAio` is only available for that operating system.
///
/// [`lio_listio`]: https://pubs.opengroup.org/onlinepubs/9699919799/functions/lio_listio.html
// Note: Unlike every other kqueue event source, POSIX AIO registers events not
// via kevent(2) but when the aiocb is submitted to the kernel via aio_read,
// aio_write, etc.  It needs the kqueue's file descriptor to do that.  So
// AsyncFd can't be used for POSIX AIO.
//
// Note that PollAio doesn't implement Drop.  There's no need.  Unlike other
// kqueue sources, there is nothing to deregister.
#[cfg_attr(docsrs, doc(cfg(all(target_os = "freebsd", feature = "aio"))))]
pub struct PollAio<E: Source> {
    io: E,
    registration: Registration,
}

// ===== impl PollAio =====

impl<E: Source> PollAio<E> {
    /// Indicates to Tokio that the source is no longer ready.  The internal
    /// readiness flag will be cleared, and tokio will wait for the next
    /// edge-triggered readiness notification from the OS.
    ///
    /// It is critical that this function not be called unless your code
    /// _actually observes_ that the source is _not_ ready.  The OS must
    /// deliver a subsequent notification, or this source will block
    /// forever.
    pub fn clear_ready(&self, ev: PollAioEvent) {
        self.registration.clear_readiness(ev.0)
    }

    /// Destroy the [`PollAio`] and return its inner Source
    pub fn into_inner(self) -> E {
        self.io
    }

    /// Creates a new `PollAio` suitable for use with POSIX AIO functions.
    ///
    /// It will be associated with the default reactor.  The runtime is usually
    /// set implicitly when this function is called from a future driven by a
    /// tokio runtime, otherwise runtime can be set explicitly with
    /// [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    pub fn new_for_aio(io: E) -> io::Result<Self> {
        Self::new_with_interest(io, Interest::AIO)
    }

    /// Creates a new `PollAio` suitable for use with [`lio_listio`].
    ///
    /// It will be associated with the default reactor.  The runtime is usually
    /// set implicitly when this function is called from a future driven by a
    /// tokio runtime, otherwise runtime can be set explicitly with
    /// [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    ///
    /// [`lio_listio`]: https://pubs.opengroup.org/onlinepubs/9699919799/functions/lio_listio.html
    pub fn new_for_lio(io: E) -> io::Result<Self> {
        Self::new_with_interest(io, Interest::LIO)
    }

    fn new_with_interest(mut io: E, interest: Interest) -> io::Result<Self> {
        let handle = Handle::current();
        let registration = Registration::new_with_interest_and_handle(&mut io, interest, handle)?;
        Ok(Self { io, registration })
    }

    /// Polls for readiness.  Either AIO or LIO counts.
    pub fn poll<'a>(&'a self, cx: &mut Context<'_>) -> Poll<io::Result<PollAioEvent>> {
        let ev = ready!(self.registration.poll_read_ready(cx))?;
        Poll::Ready(Ok(PollAioEvent(ev)))
    }
}

impl<E: Source> Deref for PollAio<E> {
    type Target = E;

    fn deref(&self) -> &E {
        &self.io
    }
}

impl<E: Source> DerefMut for PollAio<E> {
    fn deref_mut(&mut self) -> &mut E {
        &mut self.io
    }
}

impl<E: Source + fmt::Debug> fmt::Debug for PollAio<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PollAio").field("io", &self.io).finish()
    }
}

/// Opaque data returned by [`PollAio::poll`].
///
/// It can be fed back to [`PollAio::clear_ready`].
#[cfg_attr(docsrs, doc(cfg(all(target_os = "freebsd", feature = "aio"))))]
#[derive(Debug)]
pub struct PollAioEvent(ReadyEvent);
