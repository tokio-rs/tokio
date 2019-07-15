use crate::ucred::{self, UCred};
use bytes::{Buf, BufMut};
use futures_core::ready;
use iovec::IoVec;
use mio::Ready;
use mio_uds;
use std::convert::TryFrom;
use std::fmt;
use std::future::Future;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{self, SocketAddr};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_reactor::{Handle, PollEvented};

/// A structure representing a connected Unix socket.
///
/// This socket can be connected directly with `UnixStream::connect` or accepted
/// from a listener with `UnixListener::incoming`. Additionally, a pair of
/// anonymous Unix sockets can be created with `UnixStream::pair`.
pub struct UnixStream {
    io: PollEvented<mio_uds::UnixStream>,
}

/// Future returned by `UnixStream::connect` which will resolve to a
/// `UnixStream` when the stream is connected.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ConnectFuture {
    stream: Option<io::Result<UnixStream>>,
}

impl UnixStream {
    /// Connects to the socket named by `path`.
    ///
    /// This function will create a new Unix socket and connect to the path
    /// specified, associating the returned stream with the default event loop's
    /// handle.
    pub fn connect<P>(path: P) -> ConnectFuture
    where
        P: AsRef<Path>,
    {
        let res = mio_uds::UnixStream::connect(path).map(UnixStream::new);
        ConnectFuture { stream: Some(res) }
    }

    /// Consumes a `UnixStream` in the standard library and returns a
    /// nonblocking `UnixStream` from this crate.
    ///
    /// The returned stream will be associated with the given event loop
    /// specified by `handle` and is ready to perform I/O.
    pub fn from_std(stream: net::UnixStream, handle: &Handle) -> io::Result<UnixStream> {
        let stream = mio_uds::UnixStream::from_stream(stream)?;
        let io = PollEvented::new_with_handle(stream, handle)?;

        Ok(UnixStream { io })
    }

    /// Creates an unnamed pair of connected sockets.
    ///
    /// This function will create a pair of interconnected Unix sockets for
    /// communicating back and forth between one another. Each socket will
    /// be associated with the default event loop's handle.
    pub fn pair() -> io::Result<(UnixStream, UnixStream)> {
        let (a, b) = mio_uds::UnixStream::pair()?;
        let a = UnixStream::new(a);
        let b = UnixStream::new(b);

        Ok((a, b))
    }

    pub(crate) fn new(stream: mio_uds::UnixStream) -> UnixStream {
        let io = PollEvented::new(stream);
        UnixStream { io }
    }

    /// Test whether this socket is ready to be read or not.
    pub fn poll_read_ready(&self, cx: &mut Context<'_>, ready: Ready) -> Poll<io::Result<Ready>> {
        self.io.poll_read_ready(cx, ready)
    }

    /// Test whether this socket is ready to be written to or not.
    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Ready>> {
        self.io.poll_write_ready(cx)
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
}

impl TryFrom<UnixStream> for mio_uds::UnixStream {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`tokio_reactor::PollEvented::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    fn try_from(value: UnixStream) -> Result<Self, Self::Error> {
        value.io.into_inner()
    }
}

impl AsyncRead for UnixStream {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_read_priv(cx, buf)
    }

    fn poll_read_buf<B: BufMut>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        self.poll_read_buf_priv(cx, buf)
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
        Poll::Ready(Ok(()))
    }

    fn poll_write_buf<B: Buf>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        self.poll_write_buf_priv(cx, buf)
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
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;

        match self.io.get_ref().read(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }

    pub(crate) fn poll_read_buf_priv<B: BufMut>(
        &self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;

        let r = unsafe {
            // The `IoVec` type can't have a 0-length size, so we create a bunch
            // of dummy versions on the stack with 1 length which we'll quickly
            // overwrite.
            let b1: &mut [u8] = &mut [0];
            let b2: &mut [u8] = &mut [0];
            let b3: &mut [u8] = &mut [0];
            let b4: &mut [u8] = &mut [0];
            let b5: &mut [u8] = &mut [0];
            let b6: &mut [u8] = &mut [0];
            let b7: &mut [u8] = &mut [0];
            let b8: &mut [u8] = &mut [0];
            let b9: &mut [u8] = &mut [0];
            let b10: &mut [u8] = &mut [0];
            let b11: &mut [u8] = &mut [0];
            let b12: &mut [u8] = &mut [0];
            let b13: &mut [u8] = &mut [0];
            let b14: &mut [u8] = &mut [0];
            let b15: &mut [u8] = &mut [0];
            let b16: &mut [u8] = &mut [0];
            let mut bufs: [&mut IoVec; 16] = [
                b1.into(),
                b2.into(),
                b3.into(),
                b4.into(),
                b5.into(),
                b6.into(),
                b7.into(),
                b8.into(),
                b9.into(),
                b10.into(),
                b11.into(),
                b12.into(),
                b13.into(),
                b14.into(),
                b15.into(),
                b16.into(),
            ];
            let n = buf.bytes_vec_mut(&mut bufs);
            self.io.get_ref().read_bufs(&mut bufs[..n])
        };

        match r {
            Ok(n) => {
                unsafe {
                    buf.advance_mut(n);
                }
                Poll::Ready(Ok(n))
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
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

    pub(crate) fn poll_write_buf_priv<B: Buf>(
        &self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_write_ready(cx))?;

        let r = {
            // The `IoVec` type can't have a zero-length size, so create a dummy
            // version from a 1-length slice which we'll overwrite with the
            // `bytes_vec` method.
            static DUMMY: &[u8] = &[0];
            let iovec = <&IoVec>::from(DUMMY);
            let mut bufs = [iovec; 64];
            let n = buf.bytes_vec(&mut bufs);
            self.io.get_ref().write_bufs(&bufs[..n])
        };
        match r {
            Ok(n) => {
                buf.advance(n);
                Poll::Ready(Ok(n))
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
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

impl Future for ConnectFuture {
    type Output = io::Result<UnixStream>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let stream = self
            .stream
            .take()
            .expect("ConnectFuture polled after completion")?;

        match stream.io.poll_write_ready(cx) {
            Poll::Pending => {
                self.stream = Some(Ok(stream));
                return Poll::Pending;
            }
            Poll::Ready(Err(e)) => return Err(e).into(),
            _ => (),
        }

        if let Some(e) = stream.io.get_ref().take_error()? {
            return Err(e).into();
        }

        Ok(stream).into()
    }
}
