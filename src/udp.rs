use std::io;
use std::net::{self, SocketAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::fmt;

use futures::stream::Stream;
use futures::{Future, failed, Task, Poll};
use futures_io::{Ready, IoFuture};
use mio;

use {ReadinessStream, LoopHandle};
use event_loop::Source;

/// An I/O object representing a UDP socket.
pub struct UdpSocket {
    source: Arc<Source<mio::udp::UdpSocket>>,
    ready: ReadinessStream,
}

impl LoopHandle {
    /// Create a new UDP socket bound to the specified address.
    ///
    /// This function will create a new UDP socket and attempt to bind it to the
    /// `addr` provided. The returned future will be resolved once the socket
    /// has successfully bound. If an error happens during the binding or during
    /// the socket creation, that error will be returned to the future instead.
    pub fn udp_bind(self, addr: &SocketAddr) -> IoFuture<UdpSocket> {
        match mio::udp::UdpSocket::bind(addr) {
            Ok(udp) => UdpSocket::new(udp, self),
            Err(e) => failed(e).boxed(),
        }
    }
}

impl UdpSocket {
    fn new(socket: mio::udp::UdpSocket, handle: LoopHandle)
           -> IoFuture<UdpSocket> {
        let socket = Arc::new(Source::new(socket));
        ReadinessStream::new(handle, socket.clone()).map(|ready| {
            UdpSocket {
                source: socket,
                ready: ready,
            }
        }).boxed()
    }

    /// Creates a new `UdpSocket` from the previously bound socket provided.
    ///
    /// The socket given will be registered with the event loop that `handle` is
    /// associated with. This function requires that `socket` has previously
    /// been bound to an address to work correctly.
    ///
    /// This can be used in conjunction with net2's `UdpBuilder` interface to
    /// configure a socket before it's handed off, such as setting options like
    /// `reuse_address` or binding to multiple addresses.
    pub fn from_socket(socket: net::UdpSocket,
                       handle: LoopHandle) -> IoFuture<UdpSocket> {
        match mio::udp::UdpSocket::from_socket(socket) {
            Ok(tcp) => UdpSocket::new(tcp, handle),
            Err(e) => failed(e).boxed(),
        }
    }

    /// Returns the local address that this stream is bound to.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.source.io().local_addr()
    }

    /// Sends data on the socket to the given address. On success, returns the
    /// number of bytes written.
    ///
    /// Address type can be any implementor of `ToSocketAddrs` trait. See its
    /// documentation for concrete examples.
    pub fn send_to(&self, buf: &[u8], target: &SocketAddr) -> io::Result<usize> {
        match self.source.io().send_to(buf, target) {
            Ok(Some(n)) => Ok(n),
            Ok(None) => Err(io::Error::new(io::ErrorKind::WouldBlock, "would block")),
            Err(e) => Err(e),
        }
    }

    /// Receives data from the socket. On success, returns the number of bytes
    /// read and the address from whence the data came.
    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        match self.source.io().recv_from(buf) {
            Ok(Some(n)) => Ok(n),
            Ok(None) => Err(io::Error::new(io::ErrorKind::WouldBlock, "would block")),
            Err(e) => Err(e),
        }
    }

    /// Gets the value of the `SO_BROADCAST` option for this socket.
    ///
    /// For more information about this option, see
    /// [`set_broadcast`][link].
    ///
    /// [link]: #method.set_broadcast
    pub fn broadcast(&self) -> io::Result<bool> {
        self.source.io().broadcast()
    }

    /// Sets the value of the `SO_BROADCAST` option for this socket.
    ///
    /// When enabled, this socket is allowed to send packets to a broadcast
    /// address.
    pub fn set_broadcast(&self, on: bool) -> io::Result<()> {
        self.source.io().set_broadcast(on)
    }

    /// Gets the value of the `IP_MULTICAST_LOOP` option for this socket.
    ///
    /// For more information about this option, see
    /// [`set_multicast_loop_v4`][link].
    ///
    /// [link]: #method.set_multicast_loop_v4
    pub fn multicast_loop_v4(&self) -> io::Result<bool> {
        self.source.io().multicast_loop_v4()
    }

    /// Sets the value of the `IP_MULTICAST_LOOP` option for this socket.
    ///
    /// If enabled, multicast packets will be looped back to the local socket.
    /// Note that this may not have any affect on IPv6 sockets.
    pub fn set_multicast_loop_v4(&self, on: bool) -> io::Result<()> {
        self.source.io().set_multicast_loop_v4(on)
    }

    /// Gets the value of the `IP_MULTICAST_TTL` option for this socket.
    ///
    /// For more information about this option, see
    /// [`set_multicast_ttl_v4`][link].
    ///
    /// [link]: #method.set_multicast_ttl_v4
    pub fn multicast_ttl_v4(&self) -> io::Result<u32> {
        self.source.io().multicast_ttl_v4()
    }

    /// Sets the value of the `IP_MULTICAST_TTL` option for this socket.
    ///
    /// Indicates the time-to-live value of outgoing multicast packets for
    /// this socket. The default value is 1 which means that multicast packets
    /// don't leave the local network unless explicitly requested.
    ///
    /// Note that this may not have any affect on IPv6 sockets.
    pub fn set_multicast_ttl_v4(&self, ttl: u32) -> io::Result<()> {
        self.source.io().set_multicast_ttl_v4(ttl)
    }

    /// Gets the value of the `IPV6_MULTICAST_LOOP` option for this socket.
    ///
    /// For more information about this option, see
    /// [`set_multicast_loop_v6`][link].
    ///
    /// [link]: #method.set_multicast_loop_v6
    pub fn multicast_loop_v6(&self) -> io::Result<bool> {
        self.source.io().multicast_loop_v6()
    }

    /// Sets the value of the `IPV6_MULTICAST_LOOP` option for this socket.
    ///
    /// Controls whether this socket sees the multicast packets it sends itself.
    /// Note that this may not have any affect on IPv4 sockets.
    pub fn set_multicast_loop_v6(&self, on: bool) -> io::Result<()> {
        self.source.io().set_multicast_loop_v6(on)
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_ttl`][link].
    ///
    /// [link]: #method.set_ttl
    pub fn ttl(&self) -> io::Result<u32> {
        self.source.io().ttl()
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.source.io().set_ttl(ttl)
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
        self.source.io().join_multicast_v4(multiaddr, interface)
    }

    /// Executes an operation of the `IPV6_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// index of the interface to join/leave (or 0 to indicate any interface).
    pub fn join_multicast_v6(&self,
                             multiaddr: &Ipv6Addr,
                             interface: u32) -> io::Result<()> {
        self.source.io().join_multicast_v6(multiaddr, interface)
    }

    /// Executes an operation of the `IP_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see
    /// [`join_multicast_v4`][link].
    ///
    /// [link]: #method.join_multicast_v4
    pub fn leave_multicast_v4(&self,
                              multiaddr: &Ipv4Addr,
                              interface: &Ipv4Addr) -> io::Result<()> {
        self.source.io().leave_multicast_v4(multiaddr, interface)
    }

    /// Executes an operation of the `IPV6_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see
    /// [`join_multicast_v6`][link].
    ///
    /// [link]: #method.join_multicast_v6
    pub fn leave_multicast_v6(&self,
                              multiaddr: &Ipv6Addr,
                              interface: u32) -> io::Result<()> {
        self.source.io().leave_multicast_v6(multiaddr, interface)
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.source.io().fmt(f)
    }
}

impl Stream for UdpSocket {
    type Item = Ready;
    type Error = io::Error;

    fn poll(&mut self, task: &mut Task) -> Poll<Option<Ready>, io::Error> {
        self.ready.poll(task)
    }

    fn schedule(&mut self, task: &mut Task) {
        self.ready.schedule(task)
    }
}

#[cfg(unix)]
mod sys {
    use std::os::unix::prelude::*;
    use super::UdpSocket;

    impl AsRawFd for UdpSocket {
        fn as_raw_fd(&self) -> RawFd {
            self.source.io().as_raw_fd()
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
    //         self.source.io().as_raw_handle()
    //     }
    // }
}
