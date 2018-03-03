use std::fmt;
use std::io::{self, Read, Write};
use std::mem;
use std::net::{self, SocketAddr, Shutdown};
use std::time::Duration;

use bytes::{Buf, BufMut};
use futures::stream::Stream;
use futures::{Future, Poll, Async};
use iovec::IoVec;
use mio;
use tokio_io::{AsyncRead, AsyncWrite};

use reactor::{Handle, PollEvented2};

/// An I/O object representing a TCP socket listening for incoming connections.
///
/// This object can be converted into a stream of incoming connections for
/// various forms of processing.
pub struct TcpListener {
    io: PollEvented2<mio::net::TcpListener>,
}

/// Stream returned by the `TcpListener::incoming` function representing the
/// stream of sockets received from a listener.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct Incoming {
    inner: TcpListener,
}

impl TcpListener {
    /// Create a new TCP listener associated with this event loop.
    ///
    /// The TCP listener will bind to the provided `addr` address, if available.
    /// If the result is `Ok`, the socket has successfully bound.
    pub fn bind(addr: &SocketAddr) -> io::Result<TcpListener> {
        let l = mio::net::TcpListener::bind(addr)?;
        Ok(TcpListener::new(l))
    }

    #[deprecated(since = "0.1.2", note = "use poll_accept instead")]
    #[doc(hidden)]
    pub fn accept(&mut self) -> io::Result<(TcpStream, SocketAddr)> {
        match self.poll_accept()? {
            Async::Ready(ret) => Ok(ret),
            Async::NotReady => Err(io::ErrorKind::WouldBlock.into()),
        }
    }

    /// Attempt to accept a connection and create a new connected `TcpStream` if
    /// successful.
    ///
    /// Note that typically for simple usage it's easier to treat incoming
    /// connections as a `Stream` of `TcpStream`s with the `incoming` method
    /// below.
    ///
    /// # Return
    ///
    /// On success, returns `Ok(Async::Ready((socket, addr)))`.
    ///
    /// If the listener is not ready to accept, the method returns
    /// `Ok(Async::NotReady)` and arranges for the current task to receive a
    /// notification when the listener becomes ready to accept.
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn poll_accept(&mut self) -> Poll<(TcpStream, SocketAddr), io::Error> {
        let (io, addr) = try_ready!(self.poll_accept_std());

        let io = mio::net::TcpStream::from_stream(io)?;
        let io = PollEvented2::new(io);
        let io = TcpStream { io };

        Ok((io, addr).into())
    }

    #[deprecated(since = "0.1.2", note = "use poll_accept_std instead")]
    #[doc(hidden)]
    pub fn accept_std(&mut self) -> io::Result<(net::TcpStream, SocketAddr)> {
        match self.poll_accept_std()? {
            Async::Ready(ret) => Ok(ret),
            Async::NotReady => Err(io::ErrorKind::WouldBlock.into()),
        }
    }

    /// Attempt to accept a connection and create a new connected `TcpStream` if
    /// successful.
    ///
    /// This function is the asme as `accept` above except that it returns a
    /// `std::net::TcpStream` instead of a `tokio::net::TcpStream`. This in turn
    /// can then allow for the TCP stream to be assoiated with a different
    /// reactor than the one this `TcpListener` is associated with.
    ///
    /// # Return
    ///
    /// On success, returns `Ok(Async::Ready((socket, addr)))`.
    ///
    /// If the listener is not ready to accept, the method returns
    /// `Ok(Async::NotReady)` and arranges for the current task to receive a
    /// notification when the listener becomes ready to accept.
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn poll_accept_std(&mut self) -> Poll<(net::TcpStream, SocketAddr), io::Error> {
        try_ready!(self.io.poll_read_ready());

        match self.io.get_ref().accept_std() {
            Ok(pair) => Ok(pair.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.need_read()?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    /// Create a new TCP listener from the standard library's TCP listener.
    ///
    /// This method can be used when the `Handle::tcp_listen` method isn't
    /// sufficient because perhaps some more configuration is needed in terms of
    /// before the calls to `bind` and `listen`.
    ///
    /// This API is typically paired with the `net2` crate and the `TcpBuilder`
    /// type to build up and customize a listener before it's shipped off to the
    /// backing event loop. This allows configuration of options like
    /// `SO_REUSEPORT`, binding to multiple addresses, etc.
    ///
    /// The `addr` argument here is one of the addresses that `listener` is
    /// bound to and the listener will only be guaranteed to accept connections
    /// of the same address type currently.
    ///
    /// Finally, the `handle` argument is the event loop that this listener will
    /// be bound to.
    ///
    /// The platform specific behavior of this function looks like:
    ///
    /// * On Unix, the socket is placed into nonblocking mode and connections
    ///   can be accepted as normal
    ///
    /// * On Windows, the address is stored internally and all future accepts
    ///   will only be for the same IP version as `addr` specified. That is, if
    ///   `addr` is an IPv4 address then all sockets accepted will be IPv4 as
    ///   well (same for IPv6).
    pub fn from_std(listener: net::TcpListener, handle: &Handle)
        -> io::Result<TcpListener>
    {
        let io = mio::net::TcpListener::from_std(listener)?;
        let io = PollEvented2::new_with_handle(io, handle)?;
        Ok(TcpListener { io })
    }

    fn new(listener: mio::net::TcpListener) -> TcpListener {
        let io = PollEvented2::new(listener);
        TcpListener { io }
    }

    /// Returns the local address that this listener is bound to.
    ///
    /// This can be useful, for example, when binding to port 0 to figure out
    /// which port was actually bound.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Consumes this listener, returning a stream of the sockets this listener
    /// accepts.
    ///
    /// This method returns an implementation of the `Stream` trait which
    /// resolves to the sockets the are accepted on this listener.
    pub fn incoming(self) -> Incoming {
        Incoming { inner: self }
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
}

impl fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl Stream for Incoming {
    type Item = TcpStream;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, io::Error> {
        let (socket, _) = try_ready!(self.inner.poll_accept());
        Ok(Async::Ready(Some(socket)))
    }
}

/// An I/O object representing a TCP stream connected to a remote endpoint.
///
/// A TCP stream can either be created by connecting to an endpoint, via the
/// [`connect`] method, or by [accepting] a connection from a [listener].
///
/// [`connect`]: struct.TcpStream.html#method.connect
/// [accepting]: struct.TcpListener.html#method.accept
/// [listener]: struct.TcpListener.html
pub struct TcpStream {
    io: PollEvented2<mio::net::TcpStream>,
}

/// Future returned by `TcpStream::connect` which will resolve to a `TcpStream`
/// when the stream is connected.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct ConnectFuture {
    inner: ConnectFutureState,
}

#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
enum ConnectFutureState {
    Waiting(TcpStream),
    Error(io::Error),
    Empty,
}

impl TcpStream {
    /// Create a new TCP stream connected to the specified address.
    ///
    /// This function will create a new TCP socket and attempt to connect it to
    /// the `addr` provided. The returned future will be resolved once the
    /// stream has successfully connected, or it wil return an error if one
    /// occurs.
    pub fn connect(addr: &SocketAddr) -> ConnectFuture {
        use self::ConnectFutureState::*;

        let inner = match mio::net::TcpStream::connect(addr) {
            Ok(tcp) => Waiting(TcpStream::new(tcp)),
            Err(e) => Error(e),
        };

        ConnectFuture { inner }
    }

    fn new(connected: mio::net::TcpStream) -> TcpStream {
        let io = PollEvented2::new(connected);
        TcpStream { io }
    }

    /// Create a new `TcpStream` from a `net::TcpStream`.
    ///
    /// This function will convert a TCP stream created by the standard library
    /// to a TCP stream ready to be used with the provided event loop handle.
    /// The stream returned is associated with the event loop and ready to
    /// perform I/O.
    pub fn from_std(stream: net::TcpStream, handle: &Handle)
        -> io::Result<TcpStream>
    {
        let io = mio::net::TcpStream::from_stream(stream)?;
        let io = PollEvented2::new_with_handle(io, handle)?;

        Ok(TcpStream { io })
    }

    /// Creates a new `TcpStream` from the pending socket inside the given
    /// `std::net::TcpStream`, connecting it to the address specified.
    ///
    /// This constructor allows configuring the socket before it's actually
    /// connected, and this function will transfer ownership to the returned
    /// `TcpStream` if successful. An unconnected `TcpStream` can be created
    /// with the `net2::TcpBuilder` type (and also configured via that route).
    ///
    /// The platform specific behavior of this function looks like:
    ///
    /// * On Unix, the socket is placed into nonblocking mode and then a
    ///   `connect` call is issued.
    ///
    /// * On Windows, the address is stored internally and the connect operation
    ///   is issued when the returned `TcpStream` is registered with an event
    ///   loop. Note that on Windows you must `bind` a socket before it can be
    ///   connected, so if a custom `TcpBuilder` is used it should be bound
    ///   (perhaps to `INADDR_ANY`) before this method is called.
    pub fn connect_std(stream: net::TcpStream,
                       addr: &SocketAddr,
                       handle: &Handle)
        -> ConnectFuture
    {
        use self::ConnectFutureState::*;

        let io = mio::net::TcpStream::connect_stream(stream, addr)
            .and_then(|io| PollEvented2::new_with_handle(io, handle));

        let inner = match io {
            Ok(io) => Waiting(TcpStream { io }),
            Err(e) => Error(e),
        };

        ConnectFuture { inner: inner }
    }

    /// Returns the local address that this stream is bound to.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Returns the remote address that this stream is connected to.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().peer_addr()
    }

    #[deprecated(since = "0.1.2", note = "use poll_peek instead")]
    #[doc(hidden)]
    pub fn peek(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.poll_peek(buf)? {
            Async::Ready(n) => Ok(n),
            Async::NotReady => Err(io::ErrorKind::WouldBlock.into()),
        }
    }

    /// Receives data on the socket from the remote address to which it is
    /// connected, without removing that data from the queue. On success,
    /// returns the number of bytes peeked.
    ///
    /// Successive calls return the same data. This is accomplished by passing
    /// `MSG_PEEK` as a flag to the underlying recv system call.
    ///
    /// # Return
    ///
    /// On success, returns `Ok(Async::Ready(num_bytes_read))`.
    ///
    /// If no data is available for reading, the method returns
    /// `Ok(Async::NotReady)` and arranges for the current task to receive a
    /// notification when the socket becomes readable or is closed.
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn poll_peek(&mut self, buf: &mut [u8]) -> Poll<usize, io::Error> {
        try_ready!(self.io.poll_read_ready());

        match self.io.get_ref().peek(buf) {
            Ok(ret) => Ok(ret.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.need_read()?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O on the specified
    /// portions to return immediately with an appropriate value (see the
    /// documentation of `Shutdown`).
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.io.get_ref().shutdown(how)
    }

    /// Gets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// For more information about this option, see [`set_nodelay`].
    ///
    /// [`set_nodelay`]: #method.set_nodelay
    pub fn nodelay(&self) -> io::Result<bool> {
        self.io.get_ref().nodelay()
    }

    /// Sets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// If set, this option disables the Nagle algorithm. This means that
    /// segments are always sent as soon as possible, even if there is only a
    /// small amount of data. When not set, data is buffered until there is a
    /// sufficient amount to send out, thereby avoiding the frequent sending of
    /// small packets.
    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.io.get_ref().set_nodelay(nodelay)
    }

    /// Gets the value of the `SO_RCVBUF` option on this socket.
    ///
    /// For more information about this option, see [`set_recv_buffer_size`].
    ///
    /// [`set_recv_buffer_size`]: #tymethod.set_recv_buffer_size
    pub fn recv_buffer_size(&self) -> io::Result<usize> {
        self.io.get_ref().recv_buffer_size()
    }

    /// Sets the value of the `SO_RCVBUF` option on this socket.
    ///
    /// Changes the size of the operating system's receive buffer associated
    /// with the socket.
    pub fn set_recv_buffer_size(&self, size: usize) -> io::Result<()> {
        self.io.get_ref().set_recv_buffer_size(size)
    }

    /// Gets the value of the `SO_SNDBUF` option on this socket.
    ///
    /// For more information about this option, see [`set_send_buffer`].
    ///
    /// [`set_send_buffer`]: #tymethod.set_send_buffer
    pub fn send_buffer_size(&self) -> io::Result<usize> {
        self.io.get_ref().send_buffer_size()
    }

    /// Sets the value of the `SO_SNDBUF` option on this socket.
    ///
    /// Changes the size of the operating system's send buffer associated with
    /// the socket.
    pub fn set_send_buffer_size(&self, size: usize) -> io::Result<()> {
        self.io.get_ref().set_send_buffer_size(size)
    }

    /// Returns whether keepalive messages are enabled on this socket, and if so
    /// the duration of time between them.
    ///
    /// For more information about this option, see [`set_keepalive`].
    ///
    /// [`set_keepalive`]: #tymethod.set_keepalive
    pub fn keepalive(&self) -> io::Result<Option<Duration>> {
        self.io.get_ref().keepalive()
    }

    /// Sets whether keepalive messages are enabled to be sent on this socket.
    ///
    /// On Unix, this option will set the `SO_KEEPALIVE` as well as the
    /// `TCP_KEEPALIVE` or `TCP_KEEPIDLE` option (depending on your platform).
    /// On Windows, this will set the `SIO_KEEPALIVE_VALS` option.
    ///
    /// If `None` is specified then keepalive messages are disabled, otherwise
    /// the duration specified will be the time to remain idle before sending a
    /// TCP keepalive probe.
    ///
    /// Some platforms specify this value in seconds, so sub-second
    /// specifications may be omitted.
    pub fn set_keepalive(&self, keepalive: Option<Duration>) -> io::Result<()> {
        self.io.get_ref().set_keepalive(keepalive)
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_ttl`].
    ///
    /// [`set_ttl`]: #tymethod.set_ttl
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

    /// Reads the linger duration for this socket by getting the `SO_LINGER`
    /// option.
    ///
    /// For more information about this option, see [`set_linger`].
    ///
    /// [`set_linger`]: #tymethod.set_linger
    pub fn linger(&self) -> io::Result<Option<Duration>> {
        self.io.get_ref().linger()
    }

    /// Sets the linger duration of this socket by setting the `SO_LINGER`
    /// option.
    ///
    /// This option controls the action taken when a stream has unsent messages
    /// and the stream is closed. If `SO_LINGER` is set, the system
    /// shall block the process  until it can transmit the data or until the
    /// time expires.
    ///
    /// If `SO_LINGER` is not specified, and the stream is closed, the system
    /// handles the call in a way that allows the process to continue as quickly
    /// as possible.
    pub fn set_linger(&self, dur: Option<Duration>) -> io::Result<()> {
        self.io.get_ref().set_linger(dur)
    }
}

// ===== impl Read / Write =====

impl Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.read(buf)
    }
}

impl Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.io.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncRead for TcpStream {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        <&TcpStream>::read_buf(&mut &*self, buf)
    }
}

impl AsyncWrite for TcpStream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        <&TcpStream>::shutdown(&mut &*self)
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        <&TcpStream>::write_buf(&mut &*self, buf)
    }
}

// ===== impl Read / Write for &'a =====

impl<'a> Read for &'a TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.io).read(buf)
    }
}

impl<'a> Write for &'a TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.io).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.io).flush()
    }
}

impl<'a> AsyncRead for &'a TcpStream {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        if let Async::NotReady = self.io.poll_read_ready()? {
            return Ok(Async::NotReady)
        }

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
                b1.into(), b2.into(), b3.into(), b4.into(),
                b5.into(), b6.into(), b7.into(), b8.into(),
                b9.into(), b10.into(), b11.into(), b12.into(),
                b13.into(), b14.into(), b15.into(), b16.into(),
            ];
            let n = buf.bytes_vec_mut(&mut bufs);
            self.io.get_ref().read_bufs(&mut bufs[..n])
        };

        match r {
            Ok(n) => {
                unsafe { buf.advance_mut(n); }
                Ok(Async::Ready(n))
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.need_read()?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }
}

impl<'a> AsyncWrite for &'a TcpStream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        Ok(().into())
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        if let Async::NotReady = self.io.poll_write_ready()? {
            return Ok(Async::NotReady)
        }

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
                Ok(Async::Ready(n))
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.need_write()?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }
}

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl Future for ConnectFuture {
    type Item = TcpStream;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<TcpStream, io::Error> {
        self.inner.poll()
    }
}

impl Future for ConnectFutureState {
    type Item = TcpStream;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<TcpStream, io::Error> {
        {
            let stream = match *self {
                ConnectFutureState::Waiting(ref mut s) => s,
                ConnectFutureState::Error(_) => {
                    let e = match mem::replace(self, ConnectFutureState::Empty) {
                        ConnectFutureState::Error(e) => e,
                        _ => panic!(),
                    };
                    return Err(e)
                }
                ConnectFutureState::Empty => panic!("can't poll TCP stream twice"),
            };

            // Once we've connected, wait for the stream to be writable as
            // that's when the actual connection has been initiated. Once we're
            // writable we check for `take_socket_error` to see if the connect
            // actually hit an error or not.
            //
            // If all that succeeded then we ship everything on up.
            if let Async::NotReady = stream.io.poll_write_ready()? {
                return Ok(Async::NotReady)
            }

            if let Some(e) = try!(stream.io.get_ref().take_error()) {
                return Err(e)
            }
        }
        match mem::replace(self, ConnectFutureState::Empty) {
            ConnectFutureState::Waiting(stream) => Ok(Async::Ready(stream)),
            _ => panic!(),
        }
    }
}

#[cfg(all(unix, not(target_os = "fuchsia")))]
mod sys {
    use std::os::unix::prelude::*;
    use super::{TcpStream, TcpListener};

    impl AsRawFd for TcpStream {
        fn as_raw_fd(&self) -> RawFd {
            self.io.get_ref().as_raw_fd()
        }
    }

    impl AsRawFd for TcpListener {
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
    // use super::{TcpStream, TcpListener};
    //
    // impl AsRawHandle for TcpStream {
    //     fn as_raw_handle(&self) -> RawHandle {
    //         self.io.get_ref().as_raw_handle()
    //     }
    // }
    //
    // impl AsRawHandle for TcpListener {
    //     fn as_raw_handle(&self) -> RawHandle {
    //         self.listener.io().as_raw_handle()
    //     }
    // }
}
