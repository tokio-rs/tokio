use crate::io::driver::{Direction, Handle, ReadyEvent};
use crate::io::registration::Registration;
use crate::io::{AsyncRead, AsyncWrite, ReadBuf};

use mio::event::Evented;
use std::fmt;
use std::io::{self, Read, Write};
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{Context, Poll};

cfg_io_driver! {
    /// Associates an I/O resource that implements the [`std::io::Read`] and/or
    /// [`std::io::Write`] traits with the reactor that drives it.
    ///
    /// `PollEvented` uses [`Registration`] internally to take a type that
    /// implements [`mio::Evented`] as well as [`std::io::Read`] and or
    /// [`std::io::Write`] and associate it with a reactor that will drive it.
    ///
    /// Once the [`mio::Evented`] type is wrapped by `PollEvented`, it can be
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
    /// resource not being ready, the caller must call [`clear_read_ready`] or
    /// [`clear_write_ready`]. This clears the readiness state until a new
    /// readiness event is received.
    ///
    /// This allows the caller to implement additional functions. For example,
    /// [`TcpListener`] implements poll_accept by using [`poll_read_ready`] and
    /// [`clear_read_ready`].
    ///
    /// ## Platform-specific events
    ///
    /// `PollEvented` also allows receiving platform-specific `mio::Ready` events.
    /// These events are included as part of the read readiness event stream. The
    /// write readiness event stream is only for `Ready::writable()` events.
    ///
    /// [`std::io::Read`]: trait@std::io::Read
    /// [`std::io::Write`]: trait@std::io::Write
    /// [`AsyncRead`]: trait@AsyncRead
    /// [`AsyncWrite`]: trait@AsyncWrite
    /// [`mio::Evented`]: trait@mio::Evented
    /// [`Registration`]: struct@Registration
    /// [`TcpListener`]: struct@crate::net::TcpListener
    /// [`clear_read_ready`]: method@Self::clear_read_ready
    /// [`clear_write_ready`]: method@Self::clear_write_ready
    /// [`poll_read_ready`]: method@Self::poll_read_ready
    /// [`poll_write_ready`]: method@Self::poll_write_ready
    pub(crate) struct PollEvented<E: Evented> {
        io: Option<E>,
        registration: Registration,
    }
}

// ===== impl PollEvented =====

impl<E> PollEvented<E>
where
    E: Evented,
{
    /// Creates a new `PollEvented` associated with the default reactor.
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    #[cfg_attr(feature = "signal", allow(unused))]
    pub(crate) fn new(io: E) -> io::Result<Self> {
        PollEvented::new_with_ready(io, mio::Ready::all())
    }

    /// Creates a new `PollEvented` associated with the default reactor, for specific `mio::Ready`
    /// state. `new_with_ready` should be used over `new` when you need control over the readiness
    /// state, such as when a file descriptor only allows reads. This does not add `hup` or `error`
    /// so if you are interested in those states, you will need to add them to the readiness state
    /// passed to this function.
    ///
    /// An example to listen to read only
    ///
    /// ```rust
    /// ##[cfg(unix)]
    ///     mio::Ready::from_usize(
    ///         mio::Ready::readable().as_usize()
    ///         | mio::unix::UnixReady::error().as_usize()
    ///         | mio::unix::UnixReady::hup().as_usize()
    ///     );
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    #[cfg_attr(feature = "signal", allow(unused))]
    pub(crate) fn new_with_ready(io: E, ready: mio::Ready) -> io::Result<Self> {
        Self::new_with_ready_and_handle(io, ready, Handle::current())
    }

    pub(crate) fn new_with_ready_and_handle(
        io: E,
        ready: mio::Ready,
        handle: Handle,
    ) -> io::Result<Self> {
        let registration = Registration::new_with_ready_and_handle(&io, ready, handle)?;
        Ok(Self {
            io: Some(io),
            registration,
        })
    }

    /// Returns a shared reference to the underlying I/O object this readiness
    /// stream is wrapping.
    #[cfg(any(
        feature = "process",
        feature = "tcp",
        feature = "udp",
        feature = "uds",
        feature = "signal"
    ))]
    pub(crate) fn get_ref(&self) -> &E {
        self.io.as_ref().unwrap()
    }

    /// Returns a mutable reference to the underlying I/O object this readiness
    /// stream is wrapping.
    pub(crate) fn get_mut(&mut self) -> &mut E {
        self.io.as_mut().unwrap()
    }

    /// Consumes self, returning the inner I/O object
    ///
    /// This function will deregister the I/O resource from the reactor before
    /// returning. If the deregistration operation fails, an error is returned.
    ///
    /// Note that deregistering does not guarantee that the I/O resource can be
    /// registered with a different reactor. Some I/O resource types can only be
    /// associated with a single reactor instance for their lifetime.
    #[cfg(any(feature = "tcp", feature = "udp", feature = "uds"))]
    pub(crate) fn into_inner(mut self) -> io::Result<E> {
        let io = self.io.take().unwrap();
        self.registration.deregister(&io)?;
        Ok(io)
    }

    pub(crate) fn clear_readiness(&self, event: ReadyEvent) {
        self.registration.clear_readiness(event);
    }

    /// Checks the I/O resource's read readiness state.
    ///
    /// The mask argument allows specifying what readiness to notify on. This
    /// can be any value, including platform specific readiness, **except**
    /// `writable`. HUP is always implicitly included on platforms that support
    /// it.
    ///
    /// If the resource is not ready for a read then `Poll::Pending` is returned
    /// and the current task is notified once a new event is received.
    ///
    /// The I/O resource will remain in a read-ready state until readiness is
    /// cleared by calling [`clear_read_ready`].
    ///
    /// [`clear_read_ready`]: method@Self::clear_read_ready
    ///
    /// # Panics
    ///
    /// This function panics if:
    ///
    /// * `ready` includes writable.
    /// * called from outside of a task context.
    ///
    /// # Warning
    ///
    /// This method may not be called concurrently. It takes `&self` to allow
    /// calling it concurrently with `poll_write_ready`.
    pub(crate) fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<ReadyEvent>> {
        self.registration.poll_readiness(cx, Direction::Read)
    }

    /// Checks the I/O resource's write readiness state.
    ///
    /// This always checks for writable readiness and also checks for HUP
    /// readiness on platforms that support it.
    ///
    /// If the resource is not ready for a write then `Poll::Pending` is
    /// returned and the current task is notified once a new event is received.
    ///
    /// The I/O resource will remain in a write-ready state until readiness is
    /// cleared by calling [`clear_write_ready`].
    ///
    /// [`clear_write_ready`]: method@Self::clear_write_ready
    ///
    /// # Panics
    ///
    /// This function panics if:
    ///
    /// * `ready` contains bits besides `writable` and `hup`.
    /// * called from outside of a task context.
    ///
    /// # Warning
    ///
    /// This method may not be called concurrently. It takes `&self` to allow
    /// calling it concurrently with `poll_read_ready`.
    pub(crate) fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<ReadyEvent>> {
        self.registration.poll_readiness(cx, Direction::Write)
    }
}

cfg_io_readiness! {
    impl<E> PollEvented<E>
    where
        E: Evented,
    {
        pub(crate) async fn readiness(&self, interest: mio::Ready) -> io::Result<ReadyEvent> {
            self.registration.readiness(interest).await
        }

        pub(crate) async fn async_io<F, R>(&self, interest: mio::Ready, mut op: F) -> io::Result<R>
        where
            F: FnMut(&E) -> io::Result<R>,
        {
            loop {
                let event = self.readiness(interest).await?;

                match op(self.get_ref()) {
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        self.clear_readiness(event);
                    }
                    x => return x,
                }
            }
        }
    }
}

// ===== Read / Write impls =====

impl<E> AsyncRead for PollEvented<E>
where
    E: Evented + Read + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            let ev = ready!(self.poll_read_ready(cx))?;

            // We can't assume the `Read` won't look at the read buffer,
            // so we have to force initialization here.
            let r = (*self).get_mut().read(buf.initialize_unfilled());

            if is_wouldblock(&r) {
                self.clear_readiness(ev);
                continue;
            }

            return Poll::Ready(r.map(|n| {
                buf.add_filled(n);
            }));
        }
    }
}

impl<E> AsyncWrite for PollEvented<E>
where
    E: Evented + Write + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            let ev = ready!(self.poll_write_ready(cx))?;

            let r = (*self).get_mut().write(buf);

            if is_wouldblock(&r) {
                self.clear_readiness(ev);
                continue;
            }

            return Poll::Ready(r);
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        loop {
            let ev = ready!(self.poll_write_ready(cx))?;

            let r = (*self).get_mut().flush();

            if is_wouldblock(&r) {
                self.clear_readiness(ev);
                continue;
            }

            return Poll::Ready(r);
        }
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

impl<E: Evented + fmt::Debug> fmt::Debug for PollEvented<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PollEvented").field("io", &self.io).finish()
    }
}

impl<E: Evented> Drop for PollEvented<E> {
    fn drop(&mut self) {
        if let Some(io) = self.io.take() {
            // Ignore errors
            let _ = self.registration.deregister(&io);
        }
    }
}
