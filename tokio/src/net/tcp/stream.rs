use crate::future::poll_fn;
use crate::io::{AsyncRead, AsyncWrite, Interest, PollEvented, ReadBuf, Ready};
use crate::net::tcp::split::{split, ReadHalf, WriteHalf};
use crate::net::tcp::split_owned::{split_owned, OwnedReadHalf, OwnedWriteHalf};
use crate::net::{to_socket_addrs, ToSocketAddrs};

use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::net::{Shutdown, SocketAddr};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, FromRawSocket};

#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

cfg_net! {
    /// A TCP stream between a local and a remote socket.
    ///
    /// A TCP stream can either be created by connecting to an endpoint, via the
    /// [`connect`] method, or by [accepting] a connection from a [listener].
    ///
    /// Reading and writing to a `TcpStream` is usually done using the
    /// convenience methods found on the [`AsyncReadExt`] and [`AsyncWriteExt`]
    /// traits. Examples import these traits through [the prelude].
    ///
    /// [`connect`]: method@TcpStream::connect
    /// [accepting]: method@crate::net::TcpListener::accept
    /// [listener]: struct@crate::net::TcpListener
    /// [`AsyncReadExt`]: trait@crate::io::AsyncReadExt
    /// [`AsyncWriteExt`]: trait@crate::io::AsyncWriteExt
    /// [the prelude]: crate::prelude
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
    ///
    /// The [`write_all`] method is defined on the [`AsyncWriteExt`] trait.
    ///
    /// [`write_all`]: fn@crate::io::AsyncWriteExt::write_all
    /// [`AsyncWriteExt`]: trait@crate::io::AsyncWriteExt
    pub struct TcpStream {
        io: PollEvented<mio::net::TcpStream>,
    }
}

impl TcpStream {
    /// Opens a TCP connection to a remote host.
    ///
    /// `addr` is an address of the remote host. Anything which implements the
    /// [`ToSocketAddrs`] trait can be supplied as the address. Note that
    /// strings only implement this trait when the **`net`** feature is enabled,
    /// as strings may contain domain names that need to be resolved.
    ///
    /// If `addr` yields multiple addresses, connect will be attempted with each
    /// of the addresses until a connection is successful. If none of the
    /// addresses result in a successful connection, the error returned from the
    /// last connection attempt (the last address) is returned.
    ///
    /// [`ToSocketAddrs`]: trait@crate::net::ToSocketAddrs
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
    ///
    /// The [`write_all`] method is defined on the [`AsyncWriteExt`] trait.
    ///
    /// [`write_all`]: fn@crate::io::AsyncWriteExt::write_all
    /// [`AsyncWriteExt`]: trait@crate::io::AsyncWriteExt
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<TcpStream> {
        let addrs = to_socket_addrs(addr).await?;

        let mut last_err = None;

        for addr in addrs {
            match TcpStream::connect_addr(addr).await {
                Ok(stream) => return Ok(stream),
                Err(e) => last_err = Some(e),
            }
        }

        Err(last_err.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "could not resolve to any address",
            )
        }))
    }

    /// Establishes a connection to the specified `addr`.
    async fn connect_addr(addr: SocketAddr) -> io::Result<TcpStream> {
        let sys = mio::net::TcpStream::connect(addr)?;
        TcpStream::connect_mio(sys).await
    }

    pub(crate) async fn connect_mio(sys: mio::net::TcpStream) -> io::Result<TcpStream> {
        let stream = TcpStream::new(sys)?;

        // Once we've connected, wait for the stream to be writable as
        // that's when the actual connection has been initiated. Once we're
        // writable we check for `take_socket_error` to see if the connect
        // actually hit an error or not.
        //
        // If all that succeeded then we ship everything on up.
        poll_fn(|cx| stream.io.registration().poll_write_ready(cx)).await?;

        if let Some(e) = stream.io.take_error()? {
            return Err(e);
        }

        Ok(stream)
    }

    pub(crate) fn new(connected: mio::net::TcpStream) -> io::Result<TcpStream> {
        let io = PollEvented::new(connected)?;
        Ok(TcpStream { io })
    }

    /// Creates new `TcpStream` from a `std::net::TcpStream`.
    ///
    /// This function is intended to be used to wrap a TCP stream from the
    /// standard library in the Tokio equivalent. The conversion assumes nothing
    /// about the underlying stream; it is left up to the user to set it in
    /// non-blocking mode.
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
    ///     std_stream.set_nonblocking(true)?;
    ///     let stream = TcpStream::from_std(std_stream)?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    pub fn from_std(stream: std::net::TcpStream) -> io::Result<TcpStream> {
        let io = mio::net::TcpStream::from_std(stream);
        let io = PollEvented::new(io)?;
        Ok(TcpStream { io })
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
        self.io.local_addr()
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
        self.io.peer_addr()
    }

    /// Attempts to receive data on the socket, without removing that data from
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
    ///     let stream = TcpStream::connect("127.0.0.1:8000").await?;
    ///     let mut buf = [0; 10];
    ///
    ///     poll_fn(|cx| {
    ///         stream.poll_peek(cx, &mut buf)
    ///     }).await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn poll_peek(&self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        loop {
            let ev = ready!(self.io.registration().poll_read_ready(cx))?;

            match self.io.peek(buf) {
                Ok(ret) => return Poll::Ready(Ok(ret)),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.io.registration().clear_readiness(ev);
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
    }

    /// Wait for any of the requested ready states.
    ///
    /// This function is usually paired with `try_read()` or `try_write()`. It
    /// can be used to concurrently read / write to the same socket on a single
    /// task without splitting the socket.
    ///
    /// # Examples
    ///
    /// Concurrently read and write to the stream on the same task without
    /// splitting.
    ///
    /// ```no_run
    /// use tokio::io::Interest;
    /// use tokio::net::TcpStream;
    /// use std::error::Error;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///     loop {
    ///         let ready = stream.ready(Interest::READABLE | Interest::WRITABLE).await?;
    ///
    ///         if ready.is_readable() {
    ///             // The buffer is **not** included in the async task and will only exist
    ///             // on the stack.
    ///             let mut data = [0; 1024];
    ///             let n = stream.try_read(&mut data[..]).unwrap();
    ///
    ///             println!("GOT {:?}", &data[..n]);
    ///         }
    ///
    ///         if ready.is_writable() {
    ///             // Write some data
    ///             stream.try_write(b"hello world").unwrap();
    ///         }
    ///     }
    /// }
    /// ```
    pub async fn ready(&self, interest: Interest) -> io::Result<Ready> {
        let event = self.io.registration().readiness(interest).await?;
        Ok(event.ready)
    }

    /// Wait for the socket to become readable.
    ///
    /// This function is equivalent to `ready(Interest::READABLE)` is usually
    /// paired with `try_read()`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///     let mut msg = vec![0; 1024];
    ///
    ///     loop {
    ///         // Wait for the socket to be readable
    ///         stream.readable().await?;
    ///
    ///         // Try to read data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match stream.try_read(&mut msg) {
    ///             Ok(n) => {
    ///                 msg.truncate(n);
    ///                 break;
    ///             }
    ///             Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
    ///                 continue;
    ///             }
    ///             Err(e) => {
    ///                 return Err(e.into());
    ///             }
    ///         }
    ///     }
    ///
    ///     println!("GOT = {:?}", msg);
    ///     Ok(())
    /// }
    /// ```
    pub async fn readable(&self) -> io::Result<()> {
        self.ready(Interest::READABLE).await?;
        Ok(())
    }

    /// Polls for read readiness.
    ///
    /// This function is intended for cases where creating and pinning a future
    /// via [`readable`] is not feasible. Where possible, using [`readable`] is
    /// preferred, as this supports polling from multiple tasks at once.
    ///
    /// [`readable`]: method@Self::readable
    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.io.registration().poll_read_ready(cx).map_ok(|_| ())
    }

    /// Try to read data from the stream into the provided buffer, returning how
    /// many bytes were read.
    ///
    /// Receives any pending data from the socket but does not wait for new data
    /// to arrive. On success, returns the number of bytes read. Because
    /// `try_read()` is non-blocking, the buffer does not have to be stored by
    /// the async task and can exist entirely on the stack.
    ///
    /// Usually, [`readable()`] or [`ready()`] is used with this function.
    ///
    /// [`readable()`]: TcpStream::readable()
    /// [`ready()`]: TcpStream::ready()
    ///
    /// # Return
    ///
    /// If data is successfully read, `Ok(n)` is returned, where `n` is the
    /// number of bytes read. `Ok(n)` indicates the stream's read half is closed
    /// and will no longer yield data. If the stream is not ready to read data
    /// `Err(io::ErrorKind::WouldBlock)` is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///     loop {
    ///         // Wait for the socket to be readable
    ///         stream.readable().await?;
    ///
    ///         // Creating the buffer **after** the `await` prevents it from
    ///         // being stored in the async task.
    ///         let mut buf = [0; 4096];
    ///
    ///         // Try to read data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match stream.try_read(&mut buf) {
    ///             Ok(0) => break,
    ///             Ok(n) => {
    ///                 println!("read {} bytes", n);
    ///             }
    ///             Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
    ///                 continue;
    ///             }
    ///             Err(e) => {
    ///                 return Err(e.into());
    ///             }
    ///         }
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn try_read(&self, buf: &mut [u8]) -> io::Result<usize> {
        use std::io::Read;

        self.io
            .registration()
            .try_io(Interest::READABLE, || (&*self.io).read(buf))
    }

    /// Wait for the socket to become writable.
    ///
    /// This function is equivalent to `ready(Interest::WRITABLE)` is usually
    /// paired with `try_write()`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///     loop {
    ///         // Wait for the socket to be writable
    ///         stream.writable().await?;
    ///
    ///         // Try to write data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match stream.try_write(b"hello world") {
    ///             Ok(n) => {
    ///                 break;
    ///             }
    ///             Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
    ///                 continue;
    ///             }
    ///             Err(e) => {
    ///                 return Err(e.into());
    ///             }
    ///         }
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn writable(&self) -> io::Result<()> {
        self.ready(Interest::WRITABLE).await?;
        Ok(())
    }

    /// Polls for write readiness.
    ///
    /// This function is intended for cases where creating and pinning a future
    /// via [`writable`] is not feasible. Where possible, using [`writable`] is
    /// preferred, as this supports polling from multiple tasks at once.
    ///
    /// [`writable`]: method@Self::writable
    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.io.registration().poll_write_ready(cx).map_ok(|_| ())
    }

    /// Try to write a buffer to the stream, returning how many bytes were
    /// written.
    ///
    /// The function will attempt to write the entire contents of `buf`, but
    /// only part of the buffer may be written.
    ///
    /// This function is usually paired with `writable()`.
    ///
    /// # Return
    ///
    /// If data is successfully written, `Ok(n)` is returned, where `n` is the
    /// number of bytes written. If the stream is not ready to write data,
    /// `Err(io::ErrorKind::WouldBlock)` is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpStream;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    ///     loop {
    ///         // Wait for the socket to be writable
    ///         stream.writable().await?;
    ///
    ///         // Try to write data, this may still fail with `WouldBlock`
    ///         // if the readiness event is a false positive.
    ///         match stream.try_write(b"hello world") {
    ///             Ok(n) => {
    ///                 break;
    ///             }
    ///             Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
    ///                 continue;
    ///             }
    ///             Err(e) => {
    ///                 return Err(e.into());
    ///             }
    ///         }
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn try_write(&self, buf: &[u8]) -> io::Result<usize> {
        use std::io::Write;

        self.io
            .registration()
            .try_io(Interest::WRITABLE, || (&*self.io).write(buf))
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
    ///
    /// The [`read`] method is defined on the [`AsyncReadExt`] trait.
    ///
    /// [`read`]: fn@crate::io::AsyncReadExt::read
    /// [`AsyncReadExt`]: trait@crate::io::AsyncReadExt
    pub async fn peek(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.io
            .registration()
            .async_io(Interest::READABLE, || self.io.peek(buf))
            .await
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
        self.io.shutdown(how)
    }

    /// Gets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// For more information about this option, see [`set_nodelay`].
    ///
    /// [`set_nodelay`]: TcpStream::set_nodelay
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
        self.io.nodelay()
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
        self.io.set_nodelay(nodelay)
    }

    /// Reads the linger duration for this socket by getting the `SO_LINGER`
    /// option.
    ///
    /// For more information about this option, see [`set_linger`].
    ///
    /// [`set_linger`]: TcpStream::set_linger
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
        let mio_socket = std::mem::ManuallyDrop::new(self.to_mio());

        mio_socket.get_linger()
    }

    /// Sets the linger duration of this socket by setting the SO_LINGER option.
    ///
    /// This option controls the action taken when a stream has unsent messages and the stream is
    /// closed. If SO_LINGER is set, the system shall block the process until it can transmit the
    /// data or until the time expires.
    ///
    /// If SO_LINGER is not specified, and the stream is closed, the system handles the call in a
    /// way that allows the process to continue as quickly as possible.
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
        let mio_socket = std::mem::ManuallyDrop::new(self.to_mio());

        mio_socket.set_linger(dur)
    }

    fn to_mio(&self) -> mio::net::TcpSocket {
        #[cfg(windows)]
        {
            unsafe { mio::net::TcpSocket::from_raw_socket(self.as_raw_socket()) }
        }

        #[cfg(unix)]
        {
            unsafe { mio::net::TcpSocket::from_raw_fd(self.as_raw_fd()) }
        }
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_ttl`].
    ///
    /// [`set_ttl`]: TcpStream::set_ttl
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
        self.io.ttl()
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
        self.io.set_ttl(ttl)
    }

    // These lifetime markers also appear in the generated documentation, and make
    // it more clear that this is a *borrowed* split.
    #[allow(clippy::needless_lifetimes)]
    /// Splits a `TcpStream` into a read half and a write half, which can be used
    /// to read and write the stream concurrently.
    ///
    /// This method is more efficient than [`into_split`], but the halves cannot be
    /// moved into independently spawned tasks.
    ///
    /// [`into_split`]: TcpStream::into_split()
    pub fn split<'a>(&'a mut self) -> (ReadHalf<'a>, WriteHalf<'a>) {
        split(self)
    }

    /// Splits a `TcpStream` into a read half and a write half, which can be used
    /// to read and write the stream concurrently.
    ///
    /// Unlike [`split`], the owned halves can be moved to separate tasks, however
    /// this comes at the cost of a heap allocation.
    ///
    /// **Note:** Dropping the write half will shut down the write half of the TCP
    /// stream. This is equivalent to calling [`shutdown(Write)`] on the `TcpStream`.
    ///
    /// [`split`]: TcpStream::split()
    /// [`shutdown(Write)`]: fn@crate::net::TcpStream::shutdown
    pub fn into_split(self) -> (OwnedReadHalf, OwnedWriteHalf) {
        split_owned(self)
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
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Safety: `TcpStream::read` correctly handles reads into uninitialized memory
        unsafe { self.io.poll_read(cx, buf) }
    }

    pub(super) fn poll_write_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.io.poll_write(cx, buf)
    }

    pub(super) fn poll_write_vectored_priv(
        &self,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.io.poll_write_vectored(cx, bufs)
    }
}

impl TryFrom<std::net::TcpStream> for TcpStream {
    type Error = io::Error;

    /// Consumes stream, returning the tokio I/O object.
    ///
    /// This is equivalent to
    /// [`TcpStream::from_std(stream)`](TcpStream::from_std).
    fn try_from(stream: std::net::TcpStream) -> Result<Self, Self::Error> {
        Self::from_std(stream)
    }
}

// ===== impl Read / Write =====

impl AsyncRead for TcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll_read_priv(cx, buf)
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

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.poll_write_vectored_priv(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        // tcp flush is a no-op
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.shutdown(std::net::Shutdown::Write)?;
        Poll::Ready(Ok(()))
    }
}

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.fmt(f)
    }
}

#[cfg(unix)]
mod sys {
    use super::TcpStream;
    use std::os::unix::prelude::*;

    impl AsRawFd for TcpStream {
        fn as_raw_fd(&self) -> RawFd {
            self.io.as_raw_fd()
        }
    }
}

#[cfg(windows)]
mod sys {
    use super::TcpStream;
    use std::os::windows::prelude::*;

    impl AsRawSocket for TcpStream {
        fn as_raw_socket(&self) -> RawSocket {
            self.io.as_raw_socket()
        }
    }
}
