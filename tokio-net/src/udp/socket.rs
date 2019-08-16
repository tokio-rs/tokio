use super::split::{split, UdpSocketRecvHalf, UdpSocketSendHalf};
use crate::driver::Handle;
use crate::util::PollEvented;

use futures_core::ready;
use futures_util::future::poll_fn;
use mio;
use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::net::{self, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::task::{Context, Poll};

/// An I/O object representing a UDP socket.
pub struct UdpSocket {
    io: PollEvented<mio::net::UdpSocket>,
}

impl UdpSocket {
    /// This function will create a new UDP socket and attempt to bind it to
    /// the `addr` provided.
    pub fn bind(addr: &SocketAddr) -> io::Result<UdpSocket> {
        mio::net::UdpSocket::bind(addr).map(UdpSocket::new)
    }

    fn new(socket: mio::net::UdpSocket) -> UdpSocket {
        let io = PollEvented::new(socket);
        UdpSocket { io }
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
    pub fn from_std(socket: net::UdpSocket, handle: &Handle) -> io::Result<UdpSocket> {
        let io = mio::net::UdpSocket::from_socket(socket)?;
        let io = PollEvented::new_with_handle(io, handle)?;
        Ok(UdpSocket { io })
    }

    /// Split the `UdpSocket` into a receive half and a send half. The two parts
    /// can be used to receive and send datagrams concurrently, even from two
    /// different tasks.
    ///
    /// See the module level documenation of [`split`](super::split) for more
    /// details.
    pub fn split(self) -> (UdpSocketRecvHalf, UdpSocketSendHalf) {
        split(self)
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

    /// Returns a future that sends data on the socket to the remote address to which it is connected.
    /// On success, the future will resolve to the number of bytes written.
    ///
    /// The [`connect`] method will connect this socket to a remote address. The future
    /// will resolve to an error if the socket is not connected.
    ///
    /// [`connect`]: #method.connect
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

    /// Returns a future that sends data on the socket to the given address.
    /// On success, the future will resolve to the number of bytes written.
    ///
    /// The future will resolve to an error if the IP version of the socket does
    /// not match that of `target`.
    pub async fn send_to(&mut self, buf: &[u8], target: &SocketAddr) -> io::Result<usize> {
        poll_fn(|cx| self.poll_send_to_priv(cx, buf, target)).await
    }

    pub(crate) fn poll_send_to_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
        target: &SocketAddr,
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
    pub fn join_multicast_v4(&self, multiaddr: Ipv4Addr, interface: Ipv4Addr) -> io::Result<()> {
        self.io.get_ref().join_multicast_v4(&multiaddr, &interface)
    }

    /// Executes an operation of the `IPV6_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// index of the interface to join/leave (or 0 to indicate any interface).
    pub fn join_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> io::Result<()> {
        self.io.get_ref().join_multicast_v6(multiaddr, interface)
    }

    /// Executes an operation of the `IP_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see [`join_multicast_v4`].
    ///
    /// [`join_multicast_v4`]: #method.join_multicast_v4
    pub fn leave_multicast_v4(&self, multiaddr: Ipv4Addr, interface: Ipv4Addr) -> io::Result<()> {
        self.io.get_ref().leave_multicast_v4(&multiaddr, &interface)
    }

    /// Executes an operation of the `IPV6_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see [`join_multicast_v6`].
    ///
    /// [`join_multicast_v6`]: #method.join_multicast_v6
    pub fn leave_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> io::Result<()> {
        self.io.get_ref().leave_multicast_v6(multiaddr, interface)
    }
}

impl TryFrom<UdpSocket> for mio::net::UdpSocket {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`tokio_net::util::PollEvented::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    fn try_from(value: UdpSocket) -> Result<Self, Self::Error> {
        value.io.into_inner()
    }
}

impl TryFrom<net::UdpSocket> for UdpSocket {
    type Error = io::Error;

    /// Consumes stream, returning the tokio I/O object.
    ///
    /// This is equivalent to
    /// [`UdpSocket::from_std(stream, &Handle::default())`](UdpSocket::from_std).
    fn try_from(stream: net::UdpSocket) -> Result<Self, Self::Error> {
        Self::from_std(stream, &Handle::default())
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

#[cfg(all(unix))]
mod sys {
    use super::UdpSocket;
    use std::os::unix::prelude::*;

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
