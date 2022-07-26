use crate::io::driver::{Handle, Interest, Registration};

use mio::event::Source;
use std::fmt;
use std::io;
use std::ops::Deref;
use std::panic::{RefUnwindSafe, UnwindSafe};

cfg_io_driver! {
    /// Associates an I/O resource that implements the [`std::io::Read`] and/or
    /// [`std::io::Write`] traits with the reactor that drives it.
    ///
    /// `PollEvented` uses [`Registration`] internally to take a type that
    /// implements [`mio::event::Source`] as well as [`std::io::Read`] and or
    /// [`std::io::Write`] and associate it with a reactor that will drive it.
    ///
    /// Once the [`mio::event::Source`] type is wrapped by `PollEvented`, it can be
    /// used from within the future's execution model. As such, the
    /// `PollEvented` type provides [`AsyncRead`] and [`AsyncWrite`]
    /// implementations using the underlying I/O resource as well as readiness
    /// events provided by the reactor.
    ///
    /// **Note**: While `PollEvented` is `Sync` (if the underlying I/O type is
    /// `Sync`), the caller must ensure that there are at most two tasks that
    /// use a `PollEvented` instance concurrently. One for reading and one for
    /// writing. While violating this requirement is "safe" from a Rust memory
    /// model point of view, it will result in unexpected behavior in the form
    /// of lost notifications and tasks hanging.
    ///
    /// ## Readiness events
    ///
    /// Besides just providing [`AsyncRead`] and [`AsyncWrite`] implementations,
    /// this type also supports access to the underlying readiness event stream.
    /// While similar in function to what [`Registration`] provides, the
    /// semantics are a bit different.
    ///
    /// Two functions are provided to access the readiness events:
    /// [`poll_read_ready`] and [`poll_write_ready`]. These functions return the
    /// current readiness state of the `PollEvented` instance. If
    /// [`poll_read_ready`] indicates read readiness, immediately calling
    /// [`poll_read_ready`] again will also indicate read readiness.
    ///
    /// When the operation is attempted and is unable to succeed due to the I/O
    /// resource not being ready, the caller must call `clear_readiness`.
    /// This clears the readiness state until a new readiness event is received.
    ///
    /// This allows the caller to implement additional functions. For example,
    /// [`TcpListener`] implements poll_accept by using [`poll_read_ready`] and
    /// `clear_read_ready`.
    ///
    /// ## Platform-specific events
    ///
    /// `PollEvented` also allows receiving platform-specific `mio::Ready` events.
    /// These events are included as part of the read readiness event stream. The
    /// write readiness event stream is only for `Ready::writable()` events.
    ///
    /// [`AsyncRead`]: crate::io::AsyncRead
    /// [`AsyncWrite`]: crate::io::AsyncWrite
    /// [`TcpListener`]: crate::net::TcpListener
    /// [`poll_read_ready`]: Registration::poll_read_ready
    /// [`poll_write_ready`]: Registration::poll_write_ready
    pub(crate) struct PollEvented<E: Source> {
        io: Option<E>,
        registration: Registration,
    }
}

// ===== impl PollEvented =====

impl<E: Source> PollEvented<E> {
    /// Creates a new `PollEvented` associated with the default reactor.
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    #[track_caller]
    #[cfg_attr(feature = "signal", allow(unused))]
    pub(crate) fn new(io: E) -> io::Result<Self> {
        PollEvented::new_with_interest(io, Interest::READABLE | Interest::WRITABLE)
    }

    /// Creates a new `PollEvented` associated with the default reactor, for
    /// specific `Interest` state. `new_with_interest` should be used over `new`
    /// when you need control over the readiness state, such as when a file
    /// descriptor only allows reads. This does not add `hup` or `error` so if
    /// you are interested in those states, you will need to add them to the
    /// readiness state passed to this function.
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called from
    /// a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter)
    /// function.
    #[track_caller]
    #[cfg_attr(feature = "signal", allow(unused))]
    pub(crate) fn new_with_interest(io: E, interest: Interest) -> io::Result<Self> {
        Self::new_with_interest_and_handle(io, interest, Handle::current())
    }

    pub(crate) fn new_with_interest_and_handle(
        mut io: E,
        interest: Interest,
        handle: Handle,
    ) -> io::Result<Self> {
        let registration = Registration::new_with_interest_and_handle(&mut io, interest, handle)?;
        Ok(Self {
            io: Some(io),
            registration,
        })
    }

    /// Returns a reference to the registration.
    #[cfg(any(
        feature = "net",
        all(unix, feature = "process"),
        all(unix, feature = "signal"),
    ))]
    pub(crate) fn registration(&self) -> &Registration {
        &self.registration
    }

    /// Deregisters the inner io from the registration and returns a Result containing the inner io.
    #[cfg(any(feature = "net", feature = "process"))]
    pub(crate) fn into_inner(mut self) -> io::Result<E> {
        let mut inner = self.io.take().unwrap(); // As io shouldn't ever be None, just unwrap here.
        self.registration.deregister(&mut inner)?;
        Ok(inner)
    }
}

feature! {
    #![any(feature = "net", all(unix, feature = "process"))]

    use crate::io::ReadBuf;
    use std::task::{Context, Poll};

    impl<E: Source> PollEvented<E> {
        // Safety: The caller must ensure that `E` can read into uninitialized memory
        pub(crate) unsafe fn poll_read<'a>(
            &'a self,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>>
        where
            &'a E: io::Read + 'a,
        {
            use std::io::Read;

            loop {
                let evt = ready!(self.registration.poll_read_ready(cx))?;

                let b = &mut *(buf.unfilled_mut() as *mut [std::mem::MaybeUninit<u8>] as *mut [u8]);
                let len = b.len();

                match self.io.as_ref().unwrap().read(b) {
                    Ok(n) => {
                        // if we read a partially full buffer, this is sufficient on unix to show
                        // that the socket buffer has been drained
                        if n > 0 && (!cfg!(windows) && n < len) {
                            self.registration.clear_readiness(evt);
                        }

                        // Safety: We trust `TcpStream::read` to have filled up `n` bytes in the
                        // buffer.
                        buf.assume_init(n);
                        buf.advance(n);
                        return Poll::Ready(Ok(()));
                    },
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        self.registration.clear_readiness(evt);
                    }
                    Err(e) => return Poll::Ready(Err(e)),
                }
            }
        }

        pub(crate) fn poll_write<'a>(&'a self, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>>
        where
            &'a E: io::Write + 'a,
        {
            use std::io::Write;
            self.registration.poll_write_io(cx, || self.io.as_ref().unwrap().write(buf))
        }

        #[cfg(feature = "net")]
        pub(crate) fn poll_write_vectored<'a>(
            &'a self,
            cx: &mut Context<'_>,
            bufs: &[io::IoSlice<'_>],
        ) -> Poll<io::Result<usize>>
        where
            &'a E: io::Write + 'a,
        {
            use std::io::Write;
            self.registration.poll_write_io(cx, || self.io.as_ref().unwrap().write_vectored(bufs))
        }
    }
}

impl<E: Source> UnwindSafe for PollEvented<E> {}

impl<E: Source> RefUnwindSafe for PollEvented<E> {}

impl<E: Source> Deref for PollEvented<E> {
    type Target = E;

    fn deref(&self) -> &E {
        self.io.as_ref().unwrap()
    }
}

impl<E: Source + fmt::Debug> fmt::Debug for PollEvented<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PollEvented").field("io", &self.io).finish()
    }
}

impl<E: Source> Drop for PollEvented<E> {
    fn drop(&mut self) {
        if let Some(mut io) = self.io.take() {
            // Ignore errors
            let _ = self.registration.deregister(&mut io);
        }
    }
}
