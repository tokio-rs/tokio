use crate::future::poll_fn;
use crate::io::{AsyncRead, AsyncWrite, PollEvented, ReadBuf};
use crate::net::unix::split::{split, ReadHalf, WriteHalf};
use crate::net::unix::split_owned::{split_owned, OwnedReadHalf, OwnedWriteHalf};
use crate::net::unix::ucred::{self, UCred};

use std::convert::TryFrom;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{self, SocketAddr};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

cfg_uds! {
    /// A structure representing a connected Unix socket.
    ///
    /// This socket can be connected directly with `UnixStream::connect` or accepted
    /// from a listener with `UnixListener::incoming`. Additionally, a pair of
    /// anonymous Unix sockets can be created with `UnixStream::pair`.
    pub struct UnixStream {
        io: PollEvented<mio_uds::UnixStream>,
    }
}

impl UnixStream {
    /// Connects to the socket named by `path`.
    ///
    /// This function will create a new Unix socket and connect to the path
    /// specified, associating the returned stream with the default event loop's
    /// handle.
    pub async fn connect<P>(path: P) -> io::Result<UnixStream>
    where
        P: AsRef<Path>,
    {
        let stream = mio_uds::UnixStream::connect(path)?;
        let stream = UnixStream::new(stream)?;

        poll_fn(|cx| stream.io.poll_write_ready(cx)).await?;
        Ok(stream)
    }

    /// Consumes a `UnixStream` in the standard library and returns a
    /// nonblocking `UnixStream` from this crate.
    ///
    /// The returned stream will be associated with the given event loop
    /// specified by `handle` and is ready to perform I/O.
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Handle::enter`](crate::runtime::Handle::enter) function.
    pub fn from_std(stream: net::UnixStream) -> io::Result<UnixStream> {
        let stream = mio_uds::UnixStream::from_stream(stream)?;
        let io = PollEvented::new(stream)?;

        Ok(UnixStream { io })
    }

    /// Creates an unnamed pair of connected sockets.
    ///
    /// This function will create a pair of interconnected Unix sockets for
    /// communicating back and forth between one another. Each socket will
    /// be associated with the default event loop's handle.
    pub fn pair() -> io::Result<(UnixStream, UnixStream)> {
        let (a, b) = mio_uds::UnixStream::pair()?;
        let a = UnixStream::new(a)?;
        let b = UnixStream::new(b)?;

        Ok((a, b))
    }

    pub(crate) fn new(stream: mio_uds::UnixStream) -> io::Result<UnixStream> {
        let io = PollEvented::new(stream)?;
        Ok(UnixStream { io })
    }

    /// Returns the socket address of the local half of this connection.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Returns the socket address of the remote half of this connection.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().peer_addr()
    }

    /// Returns effective credentials of the process which called `connect` or `pair`.
    pub fn peer_cred(&self) -> io::Result<UCred> {
        ucred::get_peer_cred(self)
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.get_ref().take_error()
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O calls on the
    /// specified portions to immediately return with an appropriate value
    /// (see the documentation of `Shutdown`).
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.io.get_ref().shutdown(how)
    }

    // These lifetime markers also appear in the generated documentation, and make
    // it more clear that this is a *borrowed* split.
    #[allow(clippy::needless_lifetimes)]
    /// Split a `UnixStream` into a read half and a write half, which can be used
    /// to read and write the stream concurrently.
    ///
    /// This method is more efficient than [`into_split`], but the halves cannot be
    /// moved into independently spawned tasks.
    ///
    /// [`into_split`]: Self::into_split()
    pub fn split<'a>(&'a mut self) -> (ReadHalf<'a>, WriteHalf<'a>) {
        split(self)
    }

    /// Splits a `UnixStream` into a read half and a write half, which can be used
    /// to read and write the stream concurrently.
    ///
    /// Unlike [`split`], the owned halves can be moved to separate tasks, however
    /// this comes at the cost of a heap allocation.
    ///
    /// **Note:** Dropping the write half will shut down the write half of the
    /// stream. This is equivalent to calling [`shutdown(Write)`] on the `UnixStream`.
    ///
    /// [`split`]: Self::split()
    /// [`shutdown(Write)`]: fn@Self::shutdown
    pub fn into_split(self) -> (OwnedReadHalf, OwnedWriteHalf) {
        split_owned(self)
    }
}

impl TryFrom<UnixStream> for mio_uds::UnixStream {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`PollEvented::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    ///
    /// [`PollEvented::into_inner`]: crate::io::PollEvented::into_inner
    fn try_from(value: UnixStream) -> Result<Self, Self::Error> {
        value.io.into_inner()
    }
}

impl TryFrom<net::UnixStream> for UnixStream {
    type Error = io::Error;

    /// Consumes stream, returning the tokio I/O object.
    ///
    /// This is equivalent to
    /// [`UnixStream::from_std(stream)`](UnixStream::from_std).
    fn try_from(stream: net::UnixStream) -> io::Result<Self> {
        Self::from_std(stream)
    }
}

impl AsyncRead for UnixStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll_read_priv(cx, buf)
    }
}

impl AsyncWrite for UnixStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_write_priv(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.shutdown(std::net::Shutdown::Write)?;
        Poll::Ready(Ok(()))
    }
}

impl UnixStream {
    // == Poll IO functions that takes `&self` ==
    //
    // They are not public because (taken from the doc of `PollEvented`):
    //
    // While `PollEvented` is `Sync` (if the underlying I/O type is `Sync`), the
    // caller must ensure that there are at most two tasks that use a
    // `PollEvented` instance concurrently. One for reading and one for writing.
    // While violating this requirement is "safe" from a Rust memory model point
    // of view, it will result in unexpected behavior in the form of lost
    // notifications and tasks hanging.

    pub(crate) fn poll_read_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;

        // Safety: `UnixStream::read` will not peak at the maybe uinitialized bytes.
        let b =
            unsafe { &mut *(buf.unfilled_mut() as *mut [std::mem::MaybeUninit<u8>] as *mut [u8]) };
        match self.io.get_ref().read(b) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
            }
            Ok(n) => {
                // Safety: We trust `UnixStream::read` to have filled up `n` bytes
                // in the buffer.
                unsafe {
                    buf.assume_init(n);
                }
                buf.add_filled(n);
                Poll::Ready(Ok(()))
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    pub(crate) fn poll_write_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_write_ready(cx))?;

        match self.io.get_ref().write(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }
}

impl fmt::Debug for UnixStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> RawFd {
        self.io.get_ref().as_raw_fd()
    }
}
