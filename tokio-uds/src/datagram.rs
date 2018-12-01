use {RecvDgram, SendDgram};

use tokio_reactor::{Handle, PollEvented};

use futures::{Async, Poll};
use mio::Ready;
use mio_uds;

use std::fmt;
use std::io;
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{self, SocketAddr};
use std::path::Path;

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

    /// Test whether this socket is ready to be read or not.
    pub fn poll_read_ready(&self, ready: Ready) -> Poll<Ready, io::Error> {
        self.io.poll_read_ready(ready)
    }

    /// Test whether this socket is ready to be written to or not.
    pub fn poll_write_ready(&self) -> Poll<Ready, io::Error> {
        self.io.poll_write_ready()
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

    /// Receives data from the socket.
    ///
    /// On success, returns the number of bytes read and the address from
    /// whence the data came.
    pub fn poll_recv_from(&self, buf: &mut [u8]) -> Poll<(usize, SocketAddr), io::Error> {
        try_ready!(self.io.poll_read_ready(Ready::readable()));

        match self.io.get_ref().recv_from(buf) {
            Ok(ret) => Ok(ret.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(Ready::readable())?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    /// Receives data from the socket.
    ///
    /// On success, returns the number of bytes read.
    pub fn poll_recv(&self, buf: &mut [u8]) -> Poll<usize, io::Error> {
        try_ready!(self.io.poll_read_ready(Ready::readable()));

        match self.io.get_ref().recv(buf) {
            Ok(ret) => Ok(ret.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(Ready::readable())?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    /// Returns a future for receiving a datagram. See the documentation on RecvDgram for details.
    pub fn recv_dgram<T>(self, buf: T) -> RecvDgram<T>
    where
        T: AsMut<[u8]>,
    {
        RecvDgram::new(self, buf)
    }

    /// Sends data on the socket to the specified address.
    ///
    /// On success, returns the number of bytes written.
    pub fn poll_send_to<P>(&self, buf: &[u8], path: P) -> Poll<usize, io::Error>
    where
        P: AsRef<Path>,
    {
        try_ready!(self.io.poll_write_ready());

        match self.io.get_ref().send_to(buf, path) {
            Ok(ret) => Ok(ret.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready()?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    /// Sends data on the socket to the socket's peer.
    ///
    /// The peer address may be set by the `connect` method, and this method
    /// will return an error if the socket has not already been connected.
    ///
    /// On success, returns the number of bytes written.
    pub fn poll_send(&self, buf: &[u8]) -> Poll<usize, io::Error> {
        try_ready!(self.io.poll_write_ready());

        match self.io.get_ref().send(buf) {
            Ok(ret) => Ok(ret.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready()?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    /// Returns a future sending the data in buf to the socket at path.
    pub fn send_dgram<T, P>(self, buf: T, path: P) -> SendDgram<T, P>
    where
        T: AsRef<[u8]>,
        P: AsRef<Path>,
    {
        SendDgram::new(self, buf, path)
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl AsRawFd for UnixDatagram {
    fn as_raw_fd(&self) -> RawFd {
        self.io.get_ref().as_raw_fd()
    }
}
