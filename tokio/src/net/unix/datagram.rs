use crate::future::poll_fn;
use crate::io::PollEvented;

use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{self, SocketAddr};
use std::path::Path;
use std::task::{Context, Poll};

cfg_uds! {
    /// An I/O object representing a Unix datagram socket.
    pub struct UnixDatagram {
        io: PollEvented<mio_uds::UnixDatagram>,
    }
}

impl UnixDatagram {
    /// Creates a new `UnixDatagram` bound to the specified path.
    pub fn bind<P>(path: P) -> io::Result<UnixDatagram>
    where
        P: AsRef<Path>,
    {
        let socket = mio_uds::UnixDatagram::bind(path)?;
        UnixDatagram::new(socket)
    }

    /// Creates an unnamed pair of connected sockets.
    ///
    /// This function will create a pair of interconnected Unix sockets for
    /// communicating back and forth between one another. Each socket will
    /// be associated with the default event loop's handle.
    pub fn pair() -> io::Result<(UnixDatagram, UnixDatagram)> {
        let (a, b) = mio_uds::UnixDatagram::pair()?;
        let a = UnixDatagram::new(a)?;
        let b = UnixDatagram::new(b)?;

        Ok((a, b))
    }

    /// Consumes a `UnixDatagram` in the standard library and returns a
    /// nonblocking `UnixDatagram` from this crate.
    ///
    /// The returned datagram will be associated with the given event loop
    /// specified by `handle` and is ready to perform I/O.
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Handle::enter`](crate::runtime::Handle::enter) function.
    pub fn from_std(datagram: net::UnixDatagram) -> io::Result<UnixDatagram> {
        let socket = mio_uds::UnixDatagram::from_datagram(datagram)?;
        let io = PollEvented::new(socket)?;
        Ok(UnixDatagram { io })
    }

    fn new(socket: mio_uds::UnixDatagram) -> io::Result<UnixDatagram> {
        let io = PollEvented::new(socket)?;
        Ok(UnixDatagram { io })
    }

    /// Creates a new `UnixDatagram` which is not bound to any address.
    pub fn unbound() -> io::Result<UnixDatagram> {
        let socket = mio_uds::UnixDatagram::unbound()?;
        UnixDatagram::new(socket)
    }

    /// Connects the socket to the specified address.
    ///
    /// The `send` method may be used to send data to the specified address.
    /// `recv` and `recv_from` will only receive data from that address.
    pub fn connect<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        self.io.get_ref().connect(path)
    }

    /// Sends data on the socket to the socket's peer.
    pub async fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        poll_fn(|cx| self.poll_send_priv(cx, buf)).await
    }

    // Poll IO functions that takes `&self` are provided for the split API.
    //
    // They are not public because (taken from the doc of `PollEvented`):
    //
    // While `PollEvented` is `Sync` (if the underlying I/O type is `Sync`), the
    // caller must ensure that there are at most two tasks that use a
    // `PollEvented` instance concurrently. One for reading and one for writing.
    // While violating this requirement is "safe" from a Rust memory model point
    // of view, it will result in unexpected behavior in the form of lost
    // notifications and tasks hanging.
    pub(crate) fn poll_send_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_write_ready(cx))?;

        match self.io.get_ref().send(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }

    /// Receives data from the socket.
    pub async fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        poll_fn(|cx| self.poll_recv_priv(cx, buf)).await
    }

    pub(crate) fn poll_recv_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;

        match self.io.get_ref().recv(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }

    /// Sends data on the socket to the specified address.
    pub async fn send_to<P>(&mut self, buf: &[u8], target: P) -> io::Result<usize>
    where
        P: AsRef<Path> + Unpin,
    {
        poll_fn(|cx| self.poll_send_to_priv(cx, buf, target.as_ref())).await
    }

    pub(crate) fn poll_send_to_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
        target: &Path,
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_write_ready(cx))?;

        match self.io.get_ref().send_to(buf, target) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }

    /// Receives data from the socket.
    pub async fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        poll_fn(|cx| self.poll_recv_from_priv(cx, buf)).await
    }

    pub(crate) fn poll_recv_from_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, SocketAddr), io::Error>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;

        match self.io.get_ref().recv_from(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }

    /// Returns the local address that this socket is bound to.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Returns the address of this socket's peer.
    ///
    /// The `connect` method will connect the socket to a peer.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().peer_addr()
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

impl TryFrom<UnixDatagram> for mio_uds::UnixDatagram {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`PollEvented::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    ///
    /// [`PollEvented::into_inner`]: crate::io::PollEvented::into_inner
    fn try_from(value: UnixDatagram) -> Result<Self, Self::Error> {
        value.io.into_inner()
    }
}

impl TryFrom<net::UnixDatagram> for UnixDatagram {
    type Error = io::Error;

    /// Consumes stream, returning the tokio I/O object.
    ///
    /// This is equivalent to
    /// [`UnixDatagram::from_std(stream)`](UnixDatagram::from_std).
    fn try_from(stream: net::UnixDatagram) -> Result<Self, Self::Error> {
        Self::from_std(stream)
    }
}

impl fmt::Debug for UnixDatagram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl AsRawFd for UnixDatagram {
    fn as_raw_fd(&self) -> RawFd {
        self.io.get_ref().as_raw_fd()
    }
}
