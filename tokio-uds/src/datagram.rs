use crate::{Recv, RecvFrom, Send, SendTo};
use mio::Ready;
use mio_uds;
use std::fmt;
use std::io;
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{self, SocketAddr};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_reactor::{Handle, PollEvented};

/// An I/O object representing a Unix datagram socket.
pub struct UnixDatagram {
    io: PollEvented<mio_uds::UnixDatagram>,
}

impl UnixDatagram {
    /// Creates a new `UnixDatagram` bound to the specified path.
    pub fn bind<P>(path: P) -> io::Result<UnixDatagram>
    where
        P: AsRef<Path>,
    {
        let socket = mio_uds::UnixDatagram::bind(path)?;
        Ok(UnixDatagram::new(socket))
    }

    /// Creates an unnamed pair of connected sockets.
    ///
    /// This function will create a pair of interconnected Unix sockets for
    /// communicating back and forth between one another. Each socket will
    /// be associated with the default event loop's handle.
    pub fn pair() -> io::Result<(UnixDatagram, UnixDatagram)> {
        let (a, b) = mio_uds::UnixDatagram::pair()?;
        let a = UnixDatagram::new(a);
        let b = UnixDatagram::new(b);

        Ok((a, b))
    }

    /// Consumes a `UnixDatagram` in the standard library and returns a
    /// nonblocking `UnixDatagram` from this crate.
    ///
    /// The returned datagram will be associated with the given event loop
    /// specified by `handle` and is ready to perform I/O.
    pub fn from_std(datagram: net::UnixDatagram, handle: &Handle) -> io::Result<UnixDatagram> {
        let socket = mio_uds::UnixDatagram::from_datagram(datagram)?;
        let io = PollEvented::new_with_handle(socket, handle)?;
        Ok(UnixDatagram { io })
    }

    fn new(socket: mio_uds::UnixDatagram) -> UnixDatagram {
        let io = PollEvented::new(socket);
        UnixDatagram { io }
    }

    /// Creates a new `UnixDatagram` which is not bound to any address.
    pub fn unbound() -> io::Result<UnixDatagram> {
        let socket = mio_uds::UnixDatagram::unbound()?;
        Ok(UnixDatagram::new(socket))
    }

    /// Connects the socket to the specified address.
    ///
    /// The `send` method may be used to send data to the specified address.
    /// `recv` and `recv_from` will only receive data from that address.
    pub fn connect<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        self.io.get_ref().connect(path)
    }

    /// Returns a future that sends data on the socket to the remote address to which it is connected.
    /// On success, the future will resolve to the number of bytes written.
    ///
    /// The [`connect`] method will connect this socket to a remote address. The future
    /// will resolve to an error if the socket is not connected.
    ///
    /// [`connect`]: #method.connect
    pub fn send<'a, 'b>(&'a mut self, buf: &'b [u8]) -> Send<'a, 'b> {
        Send::new(self, buf)
    }

    /// Sends data on the socket to the remote address to which it is connected.
    ///
    /// The [`connect`] method will connect this socket to a remote address. This
    /// method will fail if the socket is not connected.
    ///
    /// [`connect`]: #method.connect
    ///
    /// # Return
    ///
    /// On success, returns `Poll::Ready(Ok(num_bytes_written))`.
    ///
    /// If the socket is not ready for writing, the method returns
    /// `Poll::Pending` and arranges for the current task to receive a
    /// notification when the socket becomes writable.
    pub fn poll_send(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_send_priv(cx, buf)
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

    /// Returns a future that receives a single datagram message on the socket from
    /// the remote address to which it is connected. On success, the future will resolve
    /// to the number of bytes read.
    ///
    /// The function must be called with valid byte array `buf` of sufficient size to
    /// hold the message bytes. If a message is too long to fit in the supplied buffer,
    /// excess bytes may be discarded.
    ///
    /// The [`connect`] method will connect this socket to a remote address. The future
    /// will fail if the socket is not connected.
    ///
    /// [`connect`]: #method.connect
    pub fn recv<'a, 'b>(&'a mut self, buf: &'b mut [u8]) -> Recv<'a, 'b> {
        Recv::new(self, buf)
    }

    /// Receives a single datagram message on the socket from the remote address to
    /// which it is connected. On success, returns the number of bytes read.
    ///
    /// The function must be called with valid byte array `buf` of sufficient size to
    /// hold the message bytes. If a message is too long to fit in the supplied buffer,
    /// excess bytes may be discarded.
    ///
    /// The [`connect`] method will connect this socket to a remote address. This
    /// method will fail if the socket is not connected.
    ///
    /// [`connect`]: #method.connect
    ///
    /// # Return
    ///
    /// On success, returns `Poll::Ready(Ok(num_bytes_read))`.
    ///
    /// If no data is available for reading, the method returns
    /// `Poll::Pending` and arranges for the current task to receive a
    /// notification when the socket becomes receivable or is closed.
    pub fn poll_recv(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_recv_priv(cx, buf)
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

    /// Returns a future that sends data on the socket to the given address.
    /// On success, the future will resolve to the number of bytes written.
    ///
    /// The future will resolve to an error if the IP version of the socket does
    /// not match that of `target`.
    pub fn send_to<'a, 'b, P>(&'a mut self, buf: &'b [u8], target: P) -> SendTo<'a, 'b, P>
    where
        P: AsRef<Path> + Unpin,
    {
        SendTo::new(self, buf, target)
    }

    /// Sends data on the socket to the given address. On success, returns the
    /// number of bytes written.
    ///
    /// This will return an error when the IP version of the local socket
    /// does not match that of `target`.
    ///
    /// # Return
    ///
    /// On success, returns `Poll::Ready(Ok(num_bytes_written))`.
    ///
    /// If the socket is not ready for writing, the method returns
    /// `Poll::Pending` and arranges for the current task to receive a
    /// notification when the socket becomes writable.
    pub fn poll_send_to<P: AsRef<Path>>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
        target: P,
    ) -> Poll<io::Result<usize>> {
        self.poll_send_to_priv(cx, buf, target.as_ref())
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

    /// Returns a future that receives a single datagram on the socket. On success,
    /// the future resolves to the number of bytes read and the origin.
    ///
    /// The function must be called with valid byte array `buf` of sufficient size
    /// to hold the message bytes. If a message is too long to fit in the supplied
    /// buffer, excess bytes may be discarded.
    pub fn recv_from<'a, 'b>(&'a mut self, buf: &'b mut [u8]) -> RecvFrom<'a, 'b> {
        RecvFrom::new(self, buf)
    }

    /// Receives data from the socket. On success, returns the number of bytes
    /// read and the address from whence the data came.
    pub fn poll_recv_from(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, SocketAddr), io::Error>> {
        self.poll_recv_from_priv(cx, buf)
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

    /// Test whether this socket is ready to be read or not.
    pub fn poll_read_ready(&self, cx: &mut Context<'_>, ready: Ready) -> Poll<io::Result<Ready>> {
        self.io.poll_read_ready(cx, ready)
    }

    /// Test whether this socket is ready to be written to or not.
    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Ready>> {
        self.io.poll_write_ready(cx)
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

    /// Shut down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O calls on the
    /// specified portions to immediately return with an appropriate value
    /// (see the documentation of `Shutdown`).
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.io.get_ref().shutdown(how)
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
