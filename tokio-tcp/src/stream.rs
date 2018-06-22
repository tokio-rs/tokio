use std::fmt;
use std::io::{self, Read, Write};
use std::mem;
use std::net::{self, SocketAddr, Shutdown};
use std::time::Duration;

use bytes::{Buf, BufMut};
use futures::{Future, Poll, Async};
use iovec::IoVec;
use mio;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_reactor::{Handle, PollEvented};

#[cfg(feature = "unstable-futures")]
use futures2;

/// An I/O object representing a TCP stream connected to a remote endpoint.
///
/// A TCP stream can either be created by connecting to an endpoint, via the
/// [`connect`] method, or by [accepting] a connection from a [listener].
///
/// [`connect`]: struct.TcpStream.html#method.connect
/// [accepting]: struct.TcpListener.html#method.accept
/// [listener]: struct.TcpListener.html
pub struct TcpStream {
    io: PollEvented<mio::net::TcpStream>,
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

    pub(crate) fn new(connected: mio::net::TcpStream) -> TcpStream {
        let io = PollEvented::new(connected);
        TcpStream { io }
    }

    /// Create a new `TcpStream` from a `net::TcpStream`.
    ///
    /// This function will convert a TCP stream created by the standard library
    /// to a TCP stream ready to be used with the provided event loop handle.
    /// Use `Handle::default()` to lazily bind to an event loop, just like `connect` does.
    pub fn from_std(stream: net::TcpStream, handle: &Handle)
        -> io::Result<TcpStream>
    {
        let io = mio::net::TcpStream::from_stream(stream)?;
        let io = PollEvented::new_with_handle(io, handle)?;

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
            .and_then(|io| PollEvented::new_with_handle(io, handle));

        let inner = match io {
            Ok(io) => Waiting(TcpStream { io }),
            Err(e) => Error(e),
        };

        ConnectFuture { inner: inner }
    }

    /// Check the TCP stream's read readiness state.
    ///
    /// The mask argument allows specifying what readiness to notify on. This
    /// can be any value, including platform specific readiness, **except**
    /// `writable`. HUP is always implicitly included on platforms that support
    /// it.
    ///
    /// If the resource is not ready for a read then `Async::NotReady` is
    /// returned and the current task is notified once a new event is received.
    ///
    /// The stream will remain in a read-ready state until calls to `poll_read`
    /// return `NotReady`.
    ///
    /// # Panics
    ///
    /// This function panics if:
    ///
    /// * `ready` includes writable.
    /// * called from outside of a task context.
    pub fn poll_read_ready(&self, mask: mio::Ready) -> Poll<mio::Ready, io::Error> {
        self.io.poll_read_ready(mask)
    }

    /// Like `poll_read_ready`, but compatible with futures 0.2
    #[cfg(feature = "unstable-futures")]
    pub fn poll_read_ready2(&self, cx: &mut futures2::task::Context, mask: mio::Ready)
        -> futures2::Poll<mio::Ready, io::Error>
    {
        self.io.poll_read_ready2(cx, mask)
    }

    /// Check the TCP stream's write readiness state.
    ///
    /// This always checks for writable readiness and also checks for HUP
    /// readiness on platforms that support it.
    ///
    /// If the resource is not ready for a write then `Async::NotReady` is
    /// returned and the current task is notified once a new event is received.
    ///
    /// The I/O resource will remain in a write-ready state until calls to
    /// `poll_write` return `NotReady`.
    ///
    /// # Panics
    ///
    /// This function panics if:
    ///
    /// * `ready` contains bits besides `writable` and `hup`.
    /// * called from outside of a task context.
    pub fn poll_write_ready(&self) -> Poll<mio::Ready, io::Error> {
        self.io.poll_write_ready()
    }

    /// Like `poll_write_ready`, but compatible with futures 0.2.
    #[cfg(feature = "unstable-futures")]
    pub fn poll_write_ready2(&self, cx: &mut futures2::task::Context)
        -> futures2::Poll<mio::Ready, io::Error>
    {
        self.io.poll_write_ready2(cx)
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
        try_ready!(self.io.poll_read_ready(mio::Ready::readable()));

        match self.io.get_ref().peek(buf) {
            Ok(ret) => Ok(ret.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(mio::Ready::readable())?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    /// Like `poll_peek` but compatible with futures 0.2
    #[cfg(feature = "unstable-futures")]
    pub fn poll_peek2(&mut self, cx: &mut futures2::task::Context, buf: &mut [u8])
        -> futures2::Poll<usize, io::Error>
    {
        if let futures2::Async::Pending = self.io.poll_read_ready2(cx, mio::Ready::readable())? {
            return Ok(futures2::Async::Pending);
        }

        match self.io.get_ref().peek(buf) {
            Ok(ret) => Ok(ret.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready2(cx, mio::Ready::readable())?;
                Ok(futures2::Async::Pending)
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

    /// Creates a new independently owned handle to the underlying socket.
    ///
    /// The returned `TcpStream` is a reference to the same stream that this
    /// object references. Both handles will read and write the same stream of
    /// data, and options set on one stream will be propagated to the other
    /// stream.
    pub fn try_clone(&self) -> io::Result<TcpStream> {
        let io = self.io.get_ref().try_clone()?;
        Ok(TcpStream::new(io))
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

#[cfg(feature = "unstable-futures")]
impl futures2::io::AsyncRead for TcpStream {
    fn poll_read(&mut self, cx: &mut futures2::task::Context, buf: &mut [u8])
        -> futures2::Poll<usize, io::Error>
    {
        futures2::io::AsyncRead::poll_read(&mut self.io, cx, buf)
    }

    fn poll_vectored_read(&mut self, cx: &mut futures2::task::Context, vec: &mut [&mut IoVec])
        -> futures2::Poll<usize, io::Error>
    {
        futures2::io::AsyncRead::poll_vectored_read(&mut &*self, cx, vec)
    }

    unsafe fn initializer(&self) -> futures2::io::Initializer {
        futures2::io::Initializer::nop()
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

#[cfg(feature = "unstable-futures")]
impl futures2::io::AsyncWrite for TcpStream {
    fn poll_write(&mut self, cx: &mut futures2::task::Context, buf: &[u8])
        -> futures2::Poll<usize, io::Error>
    {
        futures2::io::AsyncWrite::poll_write(&mut self.io, cx, buf)
    }

    fn poll_vectored_write(&mut self, cx: &mut futures2::task::Context, vec: &[&IoVec])
        -> futures2::Poll<usize, io::Error>
    {
        futures2::io::AsyncWrite::poll_vectored_write(&mut &*self, cx, vec)
    }

    fn poll_flush(&mut self, cx: &mut futures2::task::Context) -> futures2::Poll<(), io::Error> {
        futures2::io::AsyncWrite::poll_flush(&mut self.io, cx)
    }

    fn poll_close(&mut self, cx: &mut futures2::task::Context) -> futures2::Poll<(), io::Error> {
        futures2::io::AsyncWrite::poll_close(&mut self.io, cx)
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
        if let Async::NotReady = self.io.poll_read_ready(mio::Ready::readable())? {
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
                self.io.clear_read_ready(mio::Ready::readable())?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(feature = "unstable-futures")]
impl<'a> futures2::io::AsyncRead for &'a TcpStream {
    fn poll_read(&mut self, cx: &mut futures2::task::Context, buf: &mut [u8])
        -> futures2::Poll<usize, io::Error>
    {
        futures2::io::AsyncRead::poll_read(&mut &self.io, cx, buf)
    }

    fn poll_vectored_read(&mut self, cx: &mut futures2::task::Context, vec: &mut [&mut IoVec])
        -> futures2::Poll<usize, io::Error>
    {
        if let futures2::Async::Pending = self.io.poll_read_ready2(cx, mio::Ready::readable())? {
            return Ok(futures2::Async::Pending)
        }

        let r = self.io.get_ref().read_bufs(vec);

        match r {
            Ok(n) => {
                Ok(futures2::Async::Ready(n))
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready2(cx, mio::Ready::readable())?;
                Ok(futures2::Async::Pending)
            }
            Err(e) => Err(e),
        }
    }

    unsafe fn initializer(&self) -> futures2::io::Initializer {
        futures2::io::Initializer::nop()
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
                self.io.clear_write_ready()?;
                Ok(Async::NotReady)
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(feature = "unstable-futures")]
impl<'a> futures2::io::AsyncWrite for &'a TcpStream {
    fn poll_write(&mut self, cx: &mut futures2::task::Context, buf: &[u8])
        -> futures2::Poll<usize, io::Error>
    {
        futures2::io::AsyncWrite::poll_write(&mut &self.io, cx, buf)
    }

    fn poll_vectored_write(&mut self, cx: &mut futures2::task::Context, vec: &[&IoVec])
        -> futures2::Poll<usize, io::Error>
    {
        if let futures2::Async::Pending = self.io.poll_write_ready2(cx)? {
            return Ok(futures2::Async::Pending)
        }

        let r = self.io.get_ref().write_bufs(vec);

        match r {
            Ok(n) => {
                Ok(futures2::Async::Ready(n))
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready2(cx)?;
                Ok(futures2::Async::Pending)
            }
            Err(e) => Err(e),
        }
    }

    fn poll_flush(&mut self, cx: &mut futures2::task::Context) -> futures2::Poll<(), io::Error> {
        futures2::io::AsyncWrite::poll_flush(&mut &self.io, cx)
    }

    fn poll_close(&mut self, cx: &mut futures2::task::Context) -> futures2::Poll<(), io::Error> {
        futures2::io::AsyncWrite::poll_close(&mut &self.io, cx)
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

#[cfg(feature = "unstable-futures")]
impl futures2::Future for ConnectFuture {
    type Item = TcpStream;
    type Error = io::Error;

    fn poll(&mut self, cx: &mut futures2::task::Context) -> futures2::Poll<TcpStream, io::Error> {
        futures2::Future::poll(&mut self.inner, cx)
    }
}

impl ConnectFutureState {
    fn poll_inner<F>(&mut self, f: F) -> Poll<TcpStream, io::Error>
        where F: FnOnce(&mut PollEvented<mio::net::TcpStream>) -> Poll<mio::Ready, io::Error>
    {
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
            if let Async::NotReady = f(&mut stream.io)? {
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

impl Future for ConnectFutureState {
    type Item = TcpStream;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<TcpStream, io::Error> {
        self.poll_inner(|io| io.poll_write_ready())
    }
}

#[cfg(feature = "unstable-futures")]
impl futures2::Future for ConnectFutureState {
    type Item = TcpStream;
    type Error = io::Error;

    fn poll(&mut self, cx: &mut futures2::task::Context) -> futures2::Poll<TcpStream, io::Error> {
        self.poll_inner(|io| io.poll_write_ready2(cx).map(::lower_async))
            .map(::lift_async)
    }
}

#[cfg(unix)]
mod sys {
    use std::os::unix::prelude::*;
    use super::TcpStream;

    impl AsRawFd for TcpStream {
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
    // use super::TcpStream;
    //
    // impl AsRawHandle for TcpStream {
    //     fn as_raw_handle(&self) -> RawHandle {
    //         self.io.get_ref().as_raw_handle()
    //     }
    // }
}
