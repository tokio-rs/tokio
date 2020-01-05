use crate::future::poll_fn;
use crate::io::PollEvented;
use crate::io::{AsyncRead, AsyncWrite};
use crate::net::tcp::split::{split, ReadHalf, WriteHalf};
use crate::{net::ToSocketAddrs, runtime::context::ThreadContext};
use bytes::Buf;

use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::mem::MaybeUninit;
use std::net::{self, Shutdown, SocketAddr};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

cfg_tcp! {
    pub(crate) enum StreamInner {
        Mio(PollEvented<mio::net::TcpStream>),
        Sim(crate::simulation::tcp::SimTcpStream),
    }
    /// A TCP stream between a local and a remote socket.
    ///
    /// A TCP stream can either be created by connecting to an endpoint, via the
    /// [`connect`] method, or by [accepting] a connection from a [listener].
    ///
    /// [`connect`]: struct.TcpStream.html#method.connect
    /// [accepting]: struct.TcpListener.html#method.accept
    /// [listener]: struct.TcpListener.html
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    /// use tokio::prelude::*;
    /// use std::error::Error;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///     // Write some data.
    ///     stream.write_all(b"hello world!").await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    #[derive(Debug)]
    pub struct TcpStream {
        io: StreamInner,
    }
}

impl TcpStream {
    pub(crate) fn new(io: StreamInner) -> Self {
        TcpStream { io }
    }
    /// Opens a TCP connection to a remote host.
    ///
    /// `addr` is an address of the remote host. Anything which implements
    /// `ToSocketAddrs` trait can be supplied for the address.
    ///
    /// If `addr` yields multiple addresses, connect will be attempted with each
    /// of the addresses until a connection is successful. If none of the
    /// addresses result in a successful connection, the error returned from the
    /// last connection attempt (the last address) is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    /// use tokio::prelude::*;
    /// use std::error::Error;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///     // Write some data.
    ///     stream.write_all(b"hello world!").await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<TcpStream> {
        let handle = ThreadContext::io_handle().expect("no reactor");
        let addrs = addr.to_socket_addrs().await?;
        let mut last_err = None;
        for addr in addrs {
            match handle.tcp_stream_connect_addr(addr).await {
                Ok(stream) => return Ok(TcpStream::new(stream)),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "could not resolve to any addresses",
            )
        }))
    }

    /// Create a new `TcpStream` from a `std::net::TcpStream`.
    ///
    /// This function will convert a TCP stream created by the standard library
    /// to a TCP stream ready to be used with the provided event loop handle.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::error::Error;
    /// use tokio::net::TcpStream;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let std_stream = std::net::TcpStream::connect("127.0.0.1:34254")?;
    ///     let stream = TcpStream::from_std(std_stream)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn from_std(stream: net::TcpStream) -> io::Result<TcpStream> {
        let handle = ThreadContext::io_handle().expect("no reactor");
        let io = mio::net::TcpStream::from_stream(stream)?;
        let registration = handle.register_io(&io)?;
        let io = PollEvented::new(io, registration)?;
        Ok(TcpStream::new(io.into()))
    }

    // Connect a TcpStream asynchronously that may be built with a net2 TcpBuilder.
    //
    // This should be removed in favor of some in-crate TcpSocket builder API.
    #[doc(hidden)]
    pub async fn connect_std(stream: net::TcpStream, addr: &SocketAddr) -> io::Result<TcpStream> {
        let handle = ThreadContext::io_handle().expect("no reactor");

        let io = mio::net::TcpStream::connect_stream(stream, addr)?;
        let registration = handle.register_io(&io)?;
        let io = PollEvented::new(io, registration)?;
        poll_fn(|cx| io.poll_write_ready(cx)).await?;
        if let Some(e) = io.get_ref().take_error()? {
            return Err(e);
        }
        Ok(TcpStream { io: io.into() })
    }

    pub(crate) fn from_mio(stream: mio::net::TcpStream) -> io::Result<TcpStream> {
        let handle = ThreadContext::io_handle().expect("no reactor");
        let registration = handle.register_io(&stream)?;
        let io = PollEvented::new(stream, registration)?;
        Ok(TcpStream { io: io.into() })
    }

    /// Returns the local address that this stream is bound to.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", stream.local_addr()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().local_addr(),
            StreamInner::Sim(s) => s.local_addr(),
        }
    }

    /// Returns the remote address that this stream is connected to.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", stream.peer_addr()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().peer_addr(),
            StreamInner::Sim(s) => s.peer_addr(),
        }
    }

    /// Attempt to receive data on the socket, without removing that data from
    /// the queue, registering the current task for wakeup if data is not yet
    /// available.
    ///
    /// # Return value
    ///
    /// The function returns:
    ///
    /// * `Poll::Pending` if data is not yet available.
    /// * `Poll::Ready(Ok(n))` if data is available. `n` is the number of bytes peeked.
    /// * `Poll::Ready(Err(e))` if an error is encountered.
    ///
    /// # Errors
    ///
    /// This function may encounter any standard I/O error except `WouldBlock`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::io;
    /// use tokio::net::TcpStream;
    ///
    /// use futures::future::poll_fn;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let mut stream = TcpStream::connect("127.0.0.1:8000").await?;
    ///     let mut buf = [0; 10];
    ///
    ///     poll_fn(|cx| {
    ///         stream.poll_peek(cx, &mut buf)
    ///     }).await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn poll_peek(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        match &mut self.io {
            StreamInner::Mio(m) => {
                ready!(m.poll_read_ready(cx, mio::Ready::readable()))?;
                match m.get_ref().peek(buf) {
                    Ok(ret) => Poll::Ready(Ok(ret)),
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        m.clear_read_ready(cx, mio::Ready::readable())?;
                        Poll::Pending
                    }
                    Err(e) => Poll::Ready(Err(e)),
                }
            }
            StreamInner::Sim(s) => s.poll_peek(cx, buf),
        }
    }

    /// Receives data on the socket from the remote address to which it is
    /// connected, without removing that data from the queue. On success,
    /// returns the number of bytes peeked.
    ///
    /// Successive calls return the same data. This is accomplished by passing
    /// `MSG_PEEK` as a flag to the underlying recv system call.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    /// use tokio::prelude::*;
    /// use std::error::Error;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///     let mut b1 = [0; 10];
    ///     let mut b2 = [0; 10];
    ///
    ///     // Peek at the data
    ///     let n = stream.peek(&mut b1).await?;
    ///
    ///     // Read the data
    ///     assert_eq!(n, stream.read(&mut b2[..n]).await?);
    ///     assert_eq!(&b1[..n], &b2[..n]);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn peek(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        poll_fn(|cx| self.poll_peek(cx, buf)).await
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O on the specified
    /// portions to return immediately with an appropriate value (see the
    /// documentation of `Shutdown`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    /// use std::error::Error;
    /// use std::net::Shutdown;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///     // Shutdown the stream
    ///     stream.shutdown(Shutdown::Write)?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().shutdown(how),
            StreamInner::Sim(s) => s.shutdown(how),
        }
    }

    /// Gets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// For more information about this option, see [`set_nodelay`].
    ///
    /// [`set_nodelay`]: #method.set_nodelay
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", stream.nodelay()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn nodelay(&self) -> io::Result<bool> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().nodelay(),
            StreamInner::Sim(s) => s.nodelay(),
        }
    }

    /// Sets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// If set, this option disables the Nagle algorithm. This means that
    /// segments are always sent as soon as possible, even if there is only a
    /// small amount of data. When not set, data is buffered until there is a
    /// sufficient amount to send out, thereby avoiding the frequent sending of
    /// small packets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.set_nodelay(true)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().set_nodelay(nodelay),
            StreamInner::Sim(s) => s.set_nodelay(nodelay),
        }
    }

    /// Gets the value of the `SO_RCVBUF` option on this socket.
    ///
    /// For more information about this option, see [`set_recv_buffer_size`].
    ///
    /// [`set_recv_buffer_size`]: #tymethod.set_recv_buffer_size
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", stream.recv_buffer_size()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn recv_buffer_size(&self) -> io::Result<usize> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().recv_buffer_size(),
            StreamInner::Sim(s) => s.recv_buffer_size(),
        }
    }

    /// Sets the value of the `SO_RCVBUF` option on this socket.
    ///
    /// Changes the size of the operating system's receive buffer associated
    /// with the socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.set_recv_buffer_size(100)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_recv_buffer_size(&self, size: usize) -> io::Result<()> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().set_recv_buffer_size(size),
            StreamInner::Sim(s) => s.set_recv_buffer_size(size),
        }
    }

    /// Gets the value of the `SO_SNDBUF` option on this socket.
    ///
    /// For more information about this option, see [`set_send_buffer`].
    ///
    /// [`set_send_buffer`]: #tymethod.set_send_buffer
    ///
    /// # Examples
    ///
    /// Returns whether keepalive messages are enabled on this socket, and if so
    /// the duration of time between them.
    ///
    /// For more information about this option, see [`set_keepalive`].
    ///
    /// [`set_keepalive`]: #tymethod.set_keepalive
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", stream.send_buffer_size()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn send_buffer_size(&self) -> io::Result<usize> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().send_buffer_size(),
            StreamInner::Sim(s) => s.send_buffer_size(),
        }
    }

    /// Sets the value of the `SO_SNDBUF` option on this socket.
    ///
    /// Changes the size of the operating system's send buffer associated with
    /// the socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.set_send_buffer_size(100)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_send_buffer_size(&self, size: usize) -> io::Result<()> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().set_send_buffer_size(size),
            StreamInner::Sim(s) => s.set_send_buffer_size(size),
        }
    }

    /// Returns whether keepalive messages are enabled on this socket, and if so
    /// the duration of time between them.
    ///
    /// For more information about this option, see [`set_keepalive`].
    ///
    /// [`set_keepalive`]: #tymethod.set_keepalive
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", stream.keepalive()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn keepalive(&self) -> io::Result<Option<Duration>> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().keepalive(),
            StreamInner::Sim(s) => s.keepalive(),
        }
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.set_keepalive(None)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_keepalive(&self, keepalive: Option<Duration>) -> io::Result<()> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().set_keepalive(keepalive),
            StreamInner::Sim(s) => s.set_keepalive(keepalive),
        }
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_ttl`].
    ///
    /// [`set_ttl`]: #tymethod.set_ttl
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", stream.ttl()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn ttl(&self) -> io::Result<u32> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().ttl(),
            StreamInner::Sim(s) => s.ttl(),
        }
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.set_ttl(123)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().set_ttl(ttl),
            StreamInner::Sim(s) => s.set_ttl(ttl),
        }
    }

    /// Reads the linger duration for this socket by getting the `SO_LINGER`
    /// option.
    ///
    /// For more information about this option, see [`set_linger`].
    ///
    /// [`set_linger`]: #tymethod.set_linger
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", stream.linger()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn linger(&self) -> io::Result<Option<Duration>> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().linger(),
            StreamInner::Sim(s) => s.linger(),
        }
    }

    /// Sets the linger duration of this socket by setting the `SO_LINGER`
    /// option.
    ///
    /// This option controls the action taken when a stream has unsent messages
    /// and the stream is closed. If `SO_LINGER` is set, the system
    /// shall block the process until it can transmit the data or until the
    /// time expires.
    ///
    /// If `SO_LINGER` is not specified, and the stream is closed, the system
    /// handles the call in a way that allows the process to continue as quickly
    /// as possible.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// stream.set_linger(None)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_linger(&self, dur: Option<Duration>) -> io::Result<()> {
        match &self.io {
            StreamInner::Mio(m) => m.get_ref().set_linger(dur),
            StreamInner::Sim(s) => s.set_linger(dur),
        }
    }

    /// Split a `TcpStream` into a read half and a write half, which can be used
    /// to read and write the stream concurrently.
    ///
    /// See the module level documenation of [`split`](super::split) for more
    /// details.
    pub fn split(&mut self) -> (ReadHalf<'_>, WriteHalf<'_>) {
        match &mut self.io {
            StreamInner::Mio(ref mut m) => split(m),
            _ => todo!("figure out how to split simulated types"),
        }
    }
}

/*
impl TryFrom<TcpStream> for mio::net::TcpStream {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`PollEvented::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    ///
    /// [`PollEvented::into_inner`]: crate::io::PollEvented::into_inner
    fn try_from(value: TcpStream) -> Result<Self, Self::Error> {
        value.io.into_inner()
    }
}*/

impl TryFrom<net::TcpStream> for TcpStream {
    type Error = io::Error;

    /// Consumes stream, returning the tokio I/O object.
    ///
    /// This is equivalent to
    /// [`TcpStream::from_std(stream)`](TcpStream::from_std).
    fn try_from(stream: net::TcpStream) -> Result<Self, Self::Error> {
        Self::from_std(stream)
    }
}

// ===== impl Read / Write =====

impl AsyncRead for TcpStream
where
    Self: Unpin,
{
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [MaybeUninit<u8>]) -> bool {
        false
    }

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        // Projection should be safe here as TcpStream should be Unpin
        let inner = unsafe { self.map_unchecked_mut(|i| &mut i.io) };
        match inner.get_mut() {
            StreamInner::Mio(mio) => mio.poll_read_priv(cx, buf),
            StreamInner::Sim(ref mut s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for TcpStream
where
    Self: Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // Projection should be safe here as TcpStream should be Unpin
        let inner = unsafe { self.map_unchecked_mut(|i| &mut i.io) };
        match inner.get_mut() {
            StreamInner::Mio(m) => m.poll_write_priv(cx, buf),
            StreamInner::Sim(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_write_buf<B: Buf>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        let inner = unsafe { self.map_unchecked_mut(|i| &mut i.io) };
        match inner.get_mut() {
            StreamInner::Mio(m) => m.poll_write_buf_priv(cx, buf),
            StreamInner::Sim(s) => Pin::new(s).poll_write_buf(cx, buf),
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let inner = unsafe { self.map_unchecked_mut(|i| &mut i.io) };
        match inner.get_mut() {
            // tcp flush is a no-op
            StreamInner::Mio(_) => Poll::Ready(Ok(())),
            StreamInner::Sim(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let inner = unsafe { self.map_unchecked_mut(|i| &mut i.io) };
        match inner.get_mut() {
            StreamInner::Mio(m) => Poll::Ready(m.shutdown(net::Shutdown::Write)),
            StreamInner::Sim(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

// ===== impl StreamInner =====

impl From<PollEvented<mio::net::TcpStream>> for StreamInner {
    fn from(item: PollEvented<mio::net::TcpStream>) -> Self {
        StreamInner::Mio(item)
    }
}

impl From<crate::simulation::tcp::SimTcpStream> for StreamInner {
    fn from(item: crate::simulation::tcp::SimTcpStream) -> Self {
        StreamInner::Sim(item)
    }
}

impl fmt::Debug for StreamInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamInner::Mio(m) => m.fmt(f),
            StreamInner::Sim(s) => s.fmt(f),
        }
    }
}
