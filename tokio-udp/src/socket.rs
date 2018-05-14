use super::{SendDgram, RecvDgram};

use std::io;
use std::net::{self, SocketAddr, Ipv4Addr, Ipv6Addr};
use std::fmt;

use futures::{Async, Poll};
use mio;

use tokio_reactor::{Handle, PollEvented};

/// An I/O object representing a UDP socket.
pub struct UdpSocket {
    io: PollEvented<mio::net::UdpSocket>,
}

impl UdpSocket {
    /// This function will create a new UDP socket and attempt to bind it to
    /// the `addr` provided.
    pub fn bind(addr: &SocketAddr) -> io::Result<UdpSocket> {
        mio::net::UdpSocket::bind(addr)
            .map(UdpSocket::new)
    }

    fn new(socket: mio::net::UdpSocket) -> UdpSocket {
        let io = PollEvented::new(socket);
        UdpSocket { io: io }
    }

    /// Creates a new `UdpSocket` from the previously bound socket provided.
    ///
    /// The socket given will be registered with the event loop that `handle`
    /// is associated with. This function requires that `socket` has previously
    /// been bound to an address to work correctly.
    ///
    /// This can be used in conjunction with net2's `UdpBuilder` interface to
    /// configure a socket before it's handed off, such as setting options like
    /// `reuse_address` or binding to multiple addresses.
    ///
    /// Use `Handle::default()` to lazily bind to an event loop, just like `bind` does.
    pub fn from_std(socket: net::UdpSocket,
                    handle: &Handle) -> io::Result<UdpSocket> {
        let io = mio::net::UdpSocket::from_socket(socket)?;
        let io = PollEvented::new_with_handle(io, handle)?;
        Ok(UdpSocket { io })
    }

    /// Returns the local address that this socket is bound to.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Connects the UDP socket setting the default destination for send() and
    /// limiting packets that are read via recv from the address specified in
    /// `addr`.
    pub fn connect(&self, addr: &SocketAddr) -> io::Result<()> {
        self.io.get_ref().connect(*addr)
    }

    #[deprecated(since = "0.1.2", note = "use poll_send instead")]
    #[doc(hidden)]
    pub fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.poll_send(buf)? {
            Async::Ready(n) => Ok(n),
            Async::NotReady => Err(io::ErrorKind::WouldBlock.into()),
        }
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
    /// On success, returns `Ok(Async::Ready(num_bytes_written))`.
    ///
    /// If the socket is not ready for writing, the method returns
    /// `Ok(Async::NotReady)` and arranges for the current task to receive a
    /// notification when the socket becomes writable.
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn poll_send(&mut self, buf: &[u8]) -> Poll<usize, io::Error> {
        try_ready!(self.io.poll_write_ready());

        match self.io.get_ref().send(buf) {
            Ok(n) => Ok(n.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready()?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    #[deprecated(since = "0.1.2", note = "use poll_recv instead")]
    #[doc(hidden)]
    pub fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.poll_recv(buf)? {
            Async::Ready(n) => Ok(n),
            Async::NotReady => Err(io::ErrorKind::WouldBlock.into()),
        }
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
    /// On success, returns `Ok(Async::Ready(num_bytes_read))`.
    ///
    /// If no data is available for reading, the method returns
    /// `Ok(Async::NotReady)` and arranges for the current task to receive a
    /// notification when the socket becomes receivable or is closed.
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn poll_recv(&mut self, buf: &mut [u8]) -> Poll<usize, io::Error> {
        try_ready!(self.io.poll_read_ready(mio::Ready::readable()));

        match self.io.get_ref().recv(buf) {
            Ok(n) => Ok(n.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(mio::Ready::readable())?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    #[deprecated(since = "0.1.2", note = "use poll_send_to instead")]
    #[doc(hidden)]
    pub fn send_to(&mut self, buf: &[u8], target: &SocketAddr) -> io::Result<usize> {
        match self.poll_send_to(buf, target)? {
            Async::Ready(n) => Ok(n),
            Async::NotReady => Err(io::ErrorKind::WouldBlock.into()),
        }
    }

    /// Sends data on the socket to the given address. On success, returns the
    /// number of bytes written.
    ///
    /// This will return an error when the IP version of the local socket
    /// does not match that of `target`.
    ///
    /// # Return
    ///
    /// On success, returns `Ok(Async::Ready(num_bytes_written))`.
    ///
    /// If the socket is not ready for writing, the method returns
    /// `Ok(Async::NotReady)` and arranges for the current task to receive a
    /// notification when the socket becomes writable.
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn poll_send_to(&mut self, buf: &[u8], target: &SocketAddr) -> Poll<usize, io::Error> {
        try_ready!(self.io.poll_write_ready());

        match self.io.get_ref().send_to(buf, target) {
            Ok(n) => Ok(n.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready()?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    /// Creates a future that will write the entire contents of the buffer
    /// `buf` provided as a datagram to this socket.
    ///
    /// The returned future will return after data has been written to the
    /// outbound socket. The future will resolve to the stream as well as the
    /// buffer (for reuse if needed).
    ///
    /// Any error which happens during writing will cause both the stream and
    /// the buffer to get destroyed. Note that failure to write the entire
    /// buffer is considered an error for the purposes of sending a datagram.
    ///
    /// The `buf` parameter here only requires the `AsRef<[u8]>` trait, which
    /// should be broadly applicable to accepting data which can be converted
    /// to a slice.
    pub fn send_dgram<T>(self, buf: T, addr: &SocketAddr) -> SendDgram<T>
        where T: AsRef<[u8]>,
    {
        SendDgram::new(self, buf, *addr)
    }

    #[deprecated(since = "0.1.2", note = "use poll_recv_from instead")]
    #[doc(hidden)]
    pub fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        match self.poll_recv_from(buf)? {
            Async::Ready(ret) => Ok(ret),
            Async::NotReady => Err(io::ErrorKind::WouldBlock.into()),
        }
    }

    /// Receives data from the socket. On success, returns the number of bytes
    /// read and the address from whence the data came.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn poll_recv_from(&mut self, buf: &mut [u8]) -> Poll<(usize, SocketAddr), io::Error> {
        try_ready!(self.io.poll_read_ready(mio::Ready::readable()));

        match self.io.get_ref().recv_from(buf) {
            Ok(n) => Ok(n.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(mio::Ready::readable())?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    /// Creates a future that receive a datagram to be written to the buffer
    /// provided.
    ///
    /// The returned future will return after a datagram has been received on
    /// this socket. The future will resolve to the socket, the buffer, the
    /// amount of data read, and the address the data was received from.
    ///
    /// An error during reading will cause the socket and buffer to get
    /// destroyed.
    ///
    /// The `buf` parameter here only requires the `AsMut<[u8]>` trait, which
    /// should be broadly applicable to accepting data which can be converted
    /// to a slice.
    pub fn recv_dgram<T>(self, buf: T) -> RecvDgram<T>
        where T: AsMut<[u8]>,
    {
        RecvDgram::new(self, buf)
    }

    /// Gets the value of the `SO_BROADCAST` option for this socket.
    ///
    /// For more information about this option, see [`set_broadcast`].
    ///
    /// [`set_broadcast`]: #method.set_broadcast
    pub fn broadcast(&self) -> io::Result<bool> {
        self.io.get_ref().broadcast()
    }

    /// Sets the value of the `SO_BROADCAST` option for this socket.
    ///
    /// When enabled, this socket is allowed to send packets to a broadcast
    /// address.
    pub fn set_broadcast(&self, on: bool) -> io::Result<()> {
        self.io.get_ref().set_broadcast(on)
    }

    /// Gets the value of the `IP_MULTICAST_LOOP` option for this socket.
    ///
    /// For more information about this option, see [`set_multicast_loop_v4`].
    ///
    /// [`set_multicast_loop_v4`]: #method.set_multicast_loop_v4
    pub fn multicast_loop_v4(&self) -> io::Result<bool> {
        self.io.get_ref().multicast_loop_v4()
    }

    /// Sets the value of the `IP_MULTICAST_LOOP` option for this socket.
    ///
    /// If enabled, multicast packets will be looped back to the local socket.
    ///
    /// # Note
    ///
    /// This may not have any affect on IPv6 sockets.
    pub fn set_multicast_loop_v4(&self, on: bool) -> io::Result<()> {
        self.io.get_ref().set_multicast_loop_v4(on)
    }

    /// Gets the value of the `IP_MULTICAST_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_multicast_ttl_v4`].
    ///
    /// [`set_multicast_ttl_v4`]: #method.set_multicast_ttl_v4
    pub fn multicast_ttl_v4(&self) -> io::Result<u32> {
        self.io.get_ref().multicast_ttl_v4()
    }

    /// Sets the value of the `IP_MULTICAST_TTL` option for this socket.
    ///
    /// Indicates the time-to-live value of outgoing multicast packets for
    /// this socket. The default value is 1 which means that multicast packets
    /// don't leave the local network unless explicitly requested.
    ///
    /// # Note
    ///
    /// This may not have any affect on IPv6 sockets.
    pub fn set_multicast_ttl_v4(&self, ttl: u32) -> io::Result<()> {
        self.io.get_ref().set_multicast_ttl_v4(ttl)
    }

    /// Gets the value of the `IPV6_MULTICAST_LOOP` option for this socket.
    ///
    /// For more information about this option, see [`set_multicast_loop_v6`].
    ///
    /// [`set_multicast_loop_v6`]: #method.set_multicast_loop_v6
    pub fn multicast_loop_v6(&self) -> io::Result<bool> {
        self.io.get_ref().multicast_loop_v6()
    }

    /// Sets the value of the `IPV6_MULTICAST_LOOP` option for this socket.
    ///
    /// Controls whether this socket sees the multicast packets it sends itself.
    ///
    /// # Note
    ///
    /// This may not have any affect on IPv4 sockets.
    pub fn set_multicast_loop_v6(&self, on: bool) -> io::Result<()> {
        self.io.get_ref().set_multicast_loop_v6(on)
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_ttl`].
    ///
    /// [`set_ttl`]: #method.set_ttl
    pub fn ttl(&self) -> io::Result<u32> {
        self.io.get_ref().ttl()
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.io.get_ref().set_ttl(ttl)
    }

    /// Executes an operation of the `IP_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// address of the local interface with which the system should join the
    /// multicast group. If it's equal to `INADDR_ANY` then an appropriate
    /// interface is chosen by the system.
    pub fn join_multicast_v4(&self,
                             multiaddr: &Ipv4Addr,
                             interface: &Ipv4Addr) -> io::Result<()> {
        self.io.get_ref().join_multicast_v4(multiaddr, interface)
    }

    /// Executes an operation of the `IPV6_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// index of the interface to join/leave (or 0 to indicate any interface).
    pub fn join_multicast_v6(&self,
                             multiaddr: &Ipv6Addr,
                             interface: u32) -> io::Result<()> {
        self.io.get_ref().join_multicast_v6(multiaddr, interface)
    }

    /// Executes an operation of the `IP_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see [`join_multicast_v4`].
    ///
    /// [`join_multicast_v4`]: #method.join_multicast_v4
    pub fn leave_multicast_v4(&self,
                              multiaddr: &Ipv4Addr,
                              interface: &Ipv4Addr) -> io::Result<()> {
        self.io.get_ref().leave_multicast_v4(multiaddr, interface)
    }

    /// Executes an operation of the `IPV6_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see [`join_multicast_v6`].
    ///
    /// [`join_multicast_v6`]: #method.join_multicast_v6
    pub fn leave_multicast_v6(&self,
                              multiaddr: &Ipv6Addr,
                              interface: u32) -> io::Result<()> {
        self.io.get_ref().leave_multicast_v6(multiaddr, interface)
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

#[cfg(all(unix))]
mod sys {
    use std::os::unix::prelude::*;
    use super::UdpSocket;

    impl AsRawFd for UdpSocket {
        fn as_raw_fd(&self) -> RawFd {
            self.io.get_ref().as_raw_fd()
        }
    }
}

#[cfg(windows)]
mod sys {
    // TODO: let's land these upstream with mio and then we can add them here.
    //
    // use std::os::windows::prelude::*;
    // use super::UdpSocket;
    //
    // impl AsRawHandle for UdpSocket {
    //     fn as_raw_handle(&self) -> RawHandle {
    //         self.io.get_ref().as_raw_handle()
    //     }
    // }
}
