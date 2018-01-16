use std::io;
use std::net::{self, SocketAddr, Ipv4Addr, Ipv6Addr};
use std::fmt;

use futures::{Async, Future, Poll};
use mio;

use reactor::{Handle, PollEvented};

/// An I/O object representing a UDP socket.
pub struct UdpSocket {
    io: PollEvented<mio::net::UdpSocket>,
}

mod frame;
pub use self::frame::{UdpFramed, UdpCodec};

impl UdpSocket {
    /// This function will create a new UDP socket and attempt to bind it to
    /// the `addr` provided.
    pub fn bind(addr: &SocketAddr) -> io::Result<UdpSocket> {
        let udp = try!(mio::net::UdpSocket::bind(addr));
        UdpSocket::new(udp, &Handle::default())
    }

    fn new(socket: mio::net::UdpSocket, handle: &Handle) -> io::Result<UdpSocket> {
        let io = try!(PollEvented::new(socket, handle));
        Ok(UdpSocket { io: io })
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
    pub fn from_std(socket: net::UdpSocket,
                    handle: &Handle) -> io::Result<UdpSocket> {
        let udp = try!(mio::net::UdpSocket::from_socket(socket));
        UdpSocket::new(udp, handle)
    }

    /// Provides a `Stream` and `Sink` interface for reading and writing to this
    /// `UdpSocket` object, using the provided `UdpCodec` to read and write the
    /// raw data.
    ///
    /// Raw UDP sockets work with datagrams, but higher-level code usually
    /// wants to batch these into meaningful chunks, called "frames". This
    /// method layers framing on top of this socket by using the `UdpCodec`
    /// trait to handle encoding and decoding of messages frames. Note that
    /// the incoming and outgoing frame types may be distinct.
    ///
    /// This function returns a *single* object that is both `Stream` and
    /// `Sink`; grouping this into a single object is often useful for layering
    /// things which require both read and write access to the underlying
    /// object.
    ///
    /// If you want to work more directly with the streams and sink, consider
    /// calling `split` on the `UdpFramed` returned by this method, which will
    /// break them into separate objects, allowing them to interact more
    /// easily.
    pub fn framed<C: UdpCodec>(self, codec: C) -> UdpFramed<C> {
        frame::new(self, codec)
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

    /// Sends data on the socket to the address previously bound via connect().
    /// On success, returns the number of bytes written.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        if let Async::NotReady = self.io.poll_write() {
            return Err(io::ErrorKind::WouldBlock.into())
        }
        match self.io.get_ref().send(buf) {
            Ok(n) => Ok(n),
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    self.io.need_write()?;
                }
                Err(e)
            }
        }
    }

    /// Receives data from the socket previously bound with connect().
    /// On success, returns the number of bytes read.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        if let Async::NotReady = self.io.poll_read() {
            return Err(io::ErrorKind::WouldBlock.into())
        }
        match self.io.get_ref().recv(buf) {
            Ok(n) => Ok(n),
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    self.io.need_read()?;
                }
                Err(e)
            }
        }
    }

    /// Test whether this socket is ready to be read or not.
    ///
    /// If the socket is *not* readable then the current task is scheduled to
    /// get a notification when the socket does become readable.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn poll_read(&self) -> Async<()> {
        self.io.poll_read()
    }

    /// Test whether this socket is ready to be written to or not.
    ///
    /// If the socket is *not* writable then the current task is scheduled to
    /// get a notification when the socket does become writable.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn poll_write(&self) -> Async<()> {
        self.io.poll_write()
    }

    /// Sends data on the socket to the given address. On success, returns the
    /// number of bytes written.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn send_to(&self, buf: &[u8], target: &SocketAddr) -> io::Result<usize> {
        if let Async::NotReady = self.io.poll_write() {
            return Err(io::ErrorKind::WouldBlock.into())
        }
        match self.io.get_ref().send_to(buf, target) {
            Ok(n) => Ok(n),
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    self.io.need_write()?;
                }
                Err(e)
            }
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
    pub fn send_dgram<T>(self, buf: T, addr: SocketAddr) -> SendDgram<T>
        where T: AsRef<[u8]>,
    {
        SendDgram::new(self, buf, addr)
    }

    /// Receives data from the socket. On success, returns the number of bytes
    /// read and the address from whence the data came.
    ///
    /// # Panics
    ///
    /// This function will panic if called outside the context of a future's
    /// task.
    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        if let Async::NotReady = self.io.poll_read() {
            return Err(io::ErrorKind::WouldBlock.into())
        }
        match self.io.get_ref().recv_from(buf) {
            Ok(n) => Ok(n),
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    self.io.need_read()?;
                }
                Err(e)
            }
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

    /// Sets the value for the `IPV6_V6ONLY` option on this socket.
    ///
    /// If this is set to `true` then the socket is restricted to sending and
    /// receiving IPv6 packets only. In this case two IPv4 and IPv6 applications
    /// can bind the same port at the same time.
    ///
    /// If this is set to `false` then the socket can be used to send and
    /// receive packets from an IPv4-mapped IPv6 address.
    pub fn set_only_v6(&self, only_v6: bool) -> io::Result<()> {
        self.io.get_ref().set_only_v6(only_v6)
    }

    /// Gets the value of the `IPV6_V6ONLY` option for this socket.
    ///
    /// For more information about this option, see [`set_only_v6`].
    ///
    /// [`set_only_v6`]: #method.set_only_v6
    pub fn only_v6(&self) -> io::Result<bool> {
        self.io.get_ref().only_v6()
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

// ===== Future SendDgram =====

/// A future used to write the entire contents of some data to a UDP socket.
///
/// This is created by the `UdpSocket::send_dgram` method.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SendDgram<T> {
    /// None means future was completed
    state: Option<SendDgramInner<T>>
}

/// A struct is used to represent the full info of SendDgram.
#[derive(Debug)]
struct SendDgramInner<T> {
    /// Tx socket
    socket: UdpSocket,
    /// The whole buffer will be sent
    buffer: T,
    /// Destination addr
    addr: SocketAddr,
}

impl<T> SendDgram<T> {
    /// Create a new future to send UDP Datagram
    fn new(socket: UdpSocket, buffer: T, addr: SocketAddr) -> SendDgram<T> {
        let inner = SendDgramInner { socket: socket, buffer: buffer, addr: addr };
        SendDgram { state: Some(inner) }
    }
}

fn incomplete_write(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, reason)
}

impl<T> Future for SendDgram<T>
    where T: AsRef<[u8]>,
{
    type Item = (UdpSocket, T);
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(UdpSocket, T), io::Error> {
        {
            let ref inner =
                self.state.as_ref().expect("SendDgram polled after completion");
            let n = try_nb!(inner.socket.send_to(inner.buffer.as_ref(), &inner.addr));
            if n != inner.buffer.as_ref().len() {
                return Err(incomplete_write("failed to send entire message \
                                             in datagram"))
            }
        }

        let inner = self.state.take().unwrap();
        Ok(Async::Ready((inner.socket, inner.buffer)))
    }
}

// ===== Future RecvDgram =====

/// A future used to receive a datagram from a UDP socket.
///
/// This is created by the `UdpSocket::recv_dgram` method.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct RecvDgram<T> {
    /// None means future was completed
    state: Option<RecvDgramInner<T>>
}

/// A struct is used to represent the full info of RecvDgram.
#[derive(Debug)]
struct RecvDgramInner<T> {
    /// Rx socket
    socket: UdpSocket,
    /// The received data will be put in the buffer
    buffer: T
}

impl<T> RecvDgram<T> {
    /// Create a new future to receive UDP Datagram
    fn new(socket: UdpSocket, buffer: T) -> RecvDgram<T> {
        let inner = RecvDgramInner { socket: socket, buffer: buffer };
        RecvDgram { state: Some(inner) }
    }
}

impl<T> Future for RecvDgram<T>
    where T: AsMut<[u8]>,
{
    type Item = (UdpSocket, T, usize, SocketAddr);
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, io::Error> {
        let (n, addr) = {
            let ref mut inner =
                self.state.as_mut().expect("RecvDgram polled after completion");

            try_nb!(inner.socket.recv_from(inner.buffer.as_mut()))
        };

        let inner = self.state.take().unwrap();
        Ok(Async::Ready((inner.socket, inner.buffer, n, addr)))
    }
}

#[cfg(all(unix, not(target_os = "fuchsia")))]
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
