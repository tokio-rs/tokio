use super::split::{
    split, split_mut, TcpStreamReadHalf, TcpStreamReadHalfMut, TcpStreamWriteHalf,
    TcpStreamWriteHalfMut,
};
use crate::driver::Handle;
use crate::util::PollEvented;

use tokio_io::{AsyncRead, AsyncWrite};

use bytes::{Buf, BufMut};
use futures_core::ready;
use futures_util::future::poll_fn;
use iovec::IoVec;
use mio;
use std::convert::TryFrom;
use std::fmt;
use std::future::Future;
use std::io::{self, Read, Write};
use std::mem;
use std::net::{self, Shutdown, SocketAddr};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

/// An I/O object representing a TCP stream connected to a remote endpoint.
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
/// #![feature(async_await)]
///
/// use tokio::net::TcpStream;
/// use tokio::prelude::*;
/// use std::error::Error;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn Error>> {
///     let addr = "127.0.0.1:8080".parse()?;
///
///     // Connect to a peer
///     let mut stream = TcpStream::connect(&addr).await?;
///
///     // Write some data.
///     stream.write_all(b"hello world!").await?;
///
///     Ok(())
/// }
/// ```
pub struct TcpStream {
    io: PollEvented<mio::net::TcpStream>,
}

/// Future returned by `TcpStream::connect` which will resolve to a `TcpStream`
/// when the stream is connected.
#[must_use = "futures do nothing unless you `.await` or poll them"]
struct ConnectFuture {
    inner: ConnectFutureState,
}

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
    /// stream has successfully connected, or it will return an error if one
    /// occurs.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use tokio::prelude::*;
    /// use std::error::Error;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let addr = "127.0.0.1:8080".parse()?;
    ///
    ///     // Connect to a peer
    ///     let mut stream = TcpStream::connect(&addr).await?;
    ///
    ///     // Write some data.
    ///     stream.write_all(b"hello world!").await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn connect(addr: &SocketAddr) -> impl Future<Output = io::Result<TcpStream>> {
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    /// use tokio_net::driver::Handle;
    ///
    /// # fn dox() -> std::io::Result<()> {
    /// let std_stream = std::net::TcpStream::connect("127.0.0.1:34254")?;
    /// let stream = TcpStream::from_std(std_stream, &Handle::default())?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_std(stream: net::TcpStream, handle: &Handle) -> io::Result<TcpStream> {
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
    pub fn connect_std(
        stream: net::TcpStream,
        addr: &SocketAddr,
        handle: &Handle,
    ) -> impl Future<Output = io::Result<TcpStream>> {
        use self::ConnectFutureState::*;

        let io = mio::net::TcpStream::connect_stream(stream, addr)
            .and_then(|io| PollEvented::new_with_handle(io, handle));

        let inner = match io {
            Ok(io) => Waiting(TcpStream { io }),
            Err(e) => Error(e),
        };

        ConnectFuture { inner }
    }

    /// Returns the local address that this stream is bound to.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// println!("{:?}", stream.local_addr()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Returns the remote address that this stream is connected to.
    /// # Examples
    ///
    /// ```no_run
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// println!("{:?}", stream.peer_addr()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().peer_addr()
    }

    fn poll_peek(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;

        match self.io.get_ref().peek(buf) {
            Ok(ret) => Poll::Ready(Ok(ret)),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
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
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use tokio::prelude::*;
    /// use std::error::Error;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let addr = "127.0.0.1:8080".parse()?;
    ///
    ///     // Connect to a peer
    ///     let mut stream = TcpStream::connect(&addr).await?;
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
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use tokio::prelude::*;
    /// use std::error::Error;
    /// use std::net::Shutdown;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let addr = "127.0.0.1:8080".parse()?;
    ///
    ///     // Connect to a peer
    ///     let mut stream = TcpStream::connect(&addr).await?;
    ///
    ///     // Shutdown the stream
    ///     stream.shutdown(Shutdown::Write)?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.io.get_ref().shutdown(how)
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
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// println!("{:?}", stream.nodelay()?);
    /// # Ok(())
    /// # }
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// stream.set_nodelay(true)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.io.get_ref().set_nodelay(nodelay)
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
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// println!("{:?}", stream.recv_buffer_size()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn recv_buffer_size(&self) -> io::Result<usize> {
        self.io.get_ref().recv_buffer_size()
    }

    /// Sets the value of the `SO_RCVBUF` option on this socket.
    ///
    /// Changes the size of the operating system's receive buffer associated
    /// with the socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// stream.set_recv_buffer_size(100)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_recv_buffer_size(&self, size: usize) -> io::Result<()> {
        self.io.get_ref().set_recv_buffer_size(size)
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
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// println!("{:?}", stream.send_buffer_size()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn send_buffer_size(&self) -> io::Result<usize> {
        self.io.get_ref().send_buffer_size()
    }

    /// Sets the value of the `SO_SNDBUF` option on this socket.
    ///
    /// Changes the size of the operating system's send buffer associated with
    /// the socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// stream.set_send_buffer_size(100)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_send_buffer_size(&self, size: usize) -> io::Result<()> {
        self.io.get_ref().set_send_buffer_size(size)
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
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// println!("{:?}", stream.keepalive()?);
    /// # Ok(())
    /// # }
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// stream.set_keepalive(None)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_keepalive(&self, keepalive: Option<Duration>) -> io::Result<()> {
        self.io.get_ref().set_keepalive(keepalive)
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
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// println!("{:?}", stream.ttl()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn ttl(&self) -> io::Result<u32> {
        self.io.get_ref().ttl()
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// stream.set_ttl(123)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.io.get_ref().set_ttl(ttl)
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
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// println!("{:?}", stream.linger()?);
    /// # Ok(())
    /// # }
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// #![feature(async_await)]
    ///
    /// use tokio::net::TcpStream;
    /// use std::net::SocketAddr;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let addr = "127.0.0.1:8080".parse()?;
    /// let stream = TcpStream::connect(&addr).await?;
    ///
    /// stream.set_linger(None)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_linger(&self, dur: Option<Duration>) -> io::Result<()> {
        self.io.get_ref().set_linger(dur)
    }

    /// Split a `TcpStream` into a read half and a write half, which can be used
    /// to read and write the stream concurrently.
    ///
    /// See the module level documenation of [`split`](super::split) for more
    /// details.
    pub fn split(self) -> (TcpStreamReadHalf, TcpStreamWriteHalf) {
        split(self)
    }

    /// Split a `TcpStream` into a read half and a write half, which can be used
    /// to read and write the stream concurrently.
    ///
    /// See the module level documenation of [`split`](super::split) for more
    /// details.
    pub fn split_mut(&mut self) -> (TcpStreamReadHalfMut<'_>, TcpStreamWriteHalfMut<'_>) {
        split_mut(self)
    }

    // == Poll IO functions that takes `&self` ==
    //
    // They are not public because (taken from the doc of `PollEvented`):
    //
    // While `PollEvented` is `Sync` (if the underlying I/O type is `Sync`), the
    // caller must ensure that there are at most two tasks that use a
    // `PollEvented` instance concurrently. One for reading and one for writing.
    // While violating this requirement is "safe" from a Rust memory model point
    // of view, it will result in unexpected behavior in the form of lost
    // notifications and tasks hanging.

    pub(crate) fn poll_read_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;

        match self.io.get_ref().read(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }

    pub(crate) fn poll_read_buf_priv<B: BufMut>(
        &self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;

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
                b1.into(),
                b2.into(),
                b3.into(),
                b4.into(),
                b5.into(),
                b6.into(),
                b7.into(),
                b8.into(),
                b9.into(),
                b10.into(),
                b11.into(),
                b12.into(),
                b13.into(),
                b14.into(),
                b15.into(),
                b16.into(),
            ];
            let n = buf.bytes_vec_mut(&mut bufs);
            self.io.get_ref().read_bufs(&mut bufs[..n])
        };

        match r {
            Ok(n) => {
                unsafe {
                    buf.advance_mut(n);
                }
                Poll::Ready(Ok(n))
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    pub(crate) fn poll_write_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_write_ready(cx))?;

        match self.io.get_ref().write(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }

    pub(crate) fn poll_write_buf_priv<B: Buf>(
        &self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_write_ready(cx))?;

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
                Poll::Ready(Ok(n))
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

impl TryFrom<TcpStream> for mio::net::TcpStream {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`tokio_net::util::PollEvented::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    fn try_from(value: TcpStream) -> Result<Self, Self::Error> {
        value.io.into_inner()
    }
}

impl TryFrom<net::TcpStream> for TcpStream {
    type Error = io::Error;

    /// Consumes stream, returning the tokio I/O object.
    ///
    /// This is equivalent to
    /// [`TcpStream::from_std(stream, &Handle::default())`](TcpStream::from_std).
    fn try_from(stream: net::TcpStream) -> Result<Self, Self::Error> {
        Self::from_std(stream, &Handle::default())
    }
}

// ===== impl Read / Write =====

impl AsyncRead for TcpStream {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_read_priv(cx, buf)
    }

    fn poll_read_buf<B: BufMut>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        self.poll_read_buf_priv(cx, buf)
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_write_priv(cx, buf)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        // tcp flush is a no-op
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_write_buf<B: Buf>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        self.poll_write_buf_priv(cx, buf)
    }
}

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl Future for ConnectFuture {
    type Output = io::Result<TcpStream>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<TcpStream>> {
        self.inner.poll_inner(|io| io.poll_write_ready(cx))
    }
}

impl ConnectFutureState {
    fn poll_inner<F>(&mut self, f: F) -> Poll<io::Result<TcpStream>>
    where
        F: FnOnce(&mut PollEvented<mio::net::TcpStream>) -> Poll<io::Result<mio::Ready>>,
    {
        {
            let stream = match *self {
                ConnectFutureState::Waiting(ref mut s) => s,
                ConnectFutureState::Error(_) => {
                    let e = match mem::replace(self, ConnectFutureState::Empty) {
                        ConnectFutureState::Error(e) => e,
                        _ => unreachable!(),
                    };
                    return Poll::Ready(Err(e));
                }
                ConnectFutureState::Empty => panic!("can't poll TCP stream twice"),
            };

            // Once we've connected, wait for the stream to be writable as
            // that's when the actual connection has been initiated. Once we're
            // writable we check for `take_socket_error` to see if the connect
            // actually hit an error or not.
            //
            // If all that succeeded then we ship everything on up.
            ready!(f(&mut stream.io))?;

            if let Some(e) = stream.io.get_ref().take_error()? {
                return Poll::Ready(Err(e));
            }
        }

        match mem::replace(self, ConnectFutureState::Empty) {
            ConnectFutureState::Waiting(stream) => Poll::Ready(Ok(stream)),
            _ => unreachable!(),
        }
    }
}

#[cfg(unix)]
mod sys {
    use super::TcpStream;
    use std::os::unix::prelude::*;

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
