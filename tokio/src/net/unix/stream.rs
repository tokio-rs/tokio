use crate::future::poll_fn;
use crate::io::{AsyncRead, AsyncWrite, Interest, PollEvented, ReadBuf, Ready};
use crate::net::unix::split::{split, ReadHalf, WriteHalf};
use crate::net::unix::split_owned::{split_owned, OwnedReadHalf, OwnedWriteHalf};
use crate::net::unix::ucred::{self, UCred};
use crate::net::unix::SocketAddr;

use std::convert::TryFrom;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

cfg_net_unix! {
    /// A structure representing a connected Unix socket.
    ///
    /// This socket can be connected directly with `UnixStream::connect` or accepted
    /// from a listener with `UnixListener::incoming`. Additionally, a pair of
    /// anonymous Unix sockets can be created with `UnixStream::pair`.
    pub struct UnixStream {
        io: PollEvented<mio::net::UnixStream>,
    }
}

impl UnixStream {
    /// Connects to the socket named by `path`.
    ///
    /// This function will create a new Unix socket and connect to the path
    /// specified, associating the returned stream with the default event loop's
    /// handle.
    pub async fn connect<P>(path: P) -> io::Result<UnixStream>
    where
        P: AsRef<Path>,
    {
        let stream = mio::net::UnixStream::connect(path)?;
        let stream = UnixStream::new(stream)?;

        poll_fn(|cx| stream.io.registration().poll_write_ready(cx)).await?;
        Ok(stream)
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
    /// use tokio::net::UnixStream;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let dir = tempfile::tempdir().unwrap();
    ///     let bind_path = dir.path().join("bind_path");
    ///     let stream = UnixStream::connect(bind_path).await?;
    ///
    ///     loop {
    ///         let ready = stream.ready(Interest::READABLE | Interest::WRITABLE).await?;
    ///
    ///         if ready.is_readable() {
    ///             let mut data = vec![0; 1024];
    ///             // Try to read data, this may still fail with `WouldBlock`
    ///             // if the readiness event is a false positive.
    ///             match stream.try_read(&mut data) {
    ///                 Ok(n) => {
    ///                     println!("read {} bytes", n);        
    ///                 }
    ///                 Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
    ///                     continue;
    ///                 }
    ///                 Err(e) => {
    ///                     return Err(e.into());
    ///                 }
    ///             }
    ///
    ///         }
    ///
    ///         if ready.is_writable() {
    ///             // Try to write data, this may still fail with `WouldBlock`
    ///             // if the readiness event is a false positive.
    ///             match stream.try_write(b"hello world") {
    ///                 Ok(n) => {
    ///                     println!("write {} bytes", n);
    ///                 }
    ///                 Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
    ///                     continue;
    ///                 }
    ///                 Err(e) => {
    ///                     return Err(e.into());
    ///                 }
    ///             }
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
    /// This function is equivalent to `ready(Interest::READABLE)` and is usually
    /// paired with `try_read()`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::UnixStream;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let dir = tempfile::tempdir().unwrap();
    ///     let bind_path = dir.path().join("bind_path");
    ///     let stream = UnixStream::connect(bind_path).await?;
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
    /// [`readable()`]: UnixStream::readable()
    /// [`ready()`]: UnixStream::ready()
    ///
    /// # Return
    ///
    /// If data is successfully read, `Ok(n)` is returned, where `n` is the
    /// number of bytes read. `Ok(0)` indicates the stream's read half is closed
    /// and will no longer yield data. If the stream is not ready to read data
    /// `Err(io::ErrorKind::WouldBlock)` is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::UnixStream;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let dir = tempfile::tempdir().unwrap();
    ///     let bind_path = dir.path().join("bind_path");
    ///     let stream = UnixStream::connect(bind_path).await?;
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
        self.io
            .registration()
            .try_io(Interest::READABLE, || (&*self.io).read(buf))
    }

    /// Wait for the socket to become writable.
    ///
    /// This function is equivalent to `ready(Interest::WRITABLE)` and is usually
    /// paired with `try_write()`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::UnixStream;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let dir = tempfile::tempdir().unwrap();
    ///     let bind_path = dir.path().join("bind_path");
    ///     let stream = UnixStream::connect(bind_path).await?;
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
    /// use tokio::net::UnixStream;
    /// use std::error::Error;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     // Connect to a peer
    ///     let dir = tempfile::tempdir().unwrap();
    ///     let bind_path = dir.path().join("bind_path");
    ///     let stream = UnixStream::connect(bind_path).await?;
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
        self.io
            .registration()
            .try_io(Interest::WRITABLE, || (&*self.io).write(buf))
    }

    /// Creates new `UnixStream` from a `std::os::unix::net::UnixStream`.
    ///
    /// This function is intended to be used to wrap a UnixStream from the
    /// standard library in the Tokio equivalent. The conversion assumes
    /// nothing about the underlying stream; it is left up to the user to set
    /// it in non-blocking mode.
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    pub fn from_std(stream: net::UnixStream) -> io::Result<UnixStream> {
        let stream = mio::net::UnixStream::from_std(stream);
        let io = PollEvented::new(stream)?;

        Ok(UnixStream { io })
    }

    /// Creates an unnamed pair of connected sockets.
    ///
    /// This function will create a pair of interconnected Unix sockets for
    /// communicating back and forth between one another. Each socket will
    /// be associated with the default event loop's handle.
    pub fn pair() -> io::Result<(UnixStream, UnixStream)> {
        let (a, b) = mio::net::UnixStream::pair()?;
        let a = UnixStream::new(a)?;
        let b = UnixStream::new(b)?;

        Ok((a, b))
    }

    pub(crate) fn new(stream: mio::net::UnixStream) -> io::Result<UnixStream> {
        let io = PollEvented::new(stream)?;
        Ok(UnixStream { io })
    }

    /// Returns the socket address of the local half of this connection.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr().map(SocketAddr)
    }

    /// Returns the socket address of the remote half of this connection.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.io.peer_addr().map(SocketAddr)
    }

    /// Returns effective credentials of the process which called `connect` or `pair`.
    pub fn peer_cred(&self) -> io::Result<UCred> {
        ucred::get_peer_cred(self)
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.take_error()
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O calls on the
    /// specified portions to immediately return with an appropriate value
    /// (see the documentation of `Shutdown`).
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.io.shutdown(how)
    }

    // These lifetime markers also appear in the generated documentation, and make
    // it more clear that this is a *borrowed* split.
    #[allow(clippy::needless_lifetimes)]
    /// Split a `UnixStream` into a read half and a write half, which can be used
    /// to read and write the stream concurrently.
    ///
    /// This method is more efficient than [`into_split`], but the halves cannot be
    /// moved into independently spawned tasks.
    ///
    /// [`into_split`]: Self::into_split()
    pub fn split<'a>(&'a mut self) -> (ReadHalf<'a>, WriteHalf<'a>) {
        split(self)
    }

    /// Splits a `UnixStream` into a read half and a write half, which can be used
    /// to read and write the stream concurrently.
    ///
    /// Unlike [`split`], the owned halves can be moved to separate tasks, however
    /// this comes at the cost of a heap allocation.
    ///
    /// **Note:** Dropping the write half will shut down the write half of the
    /// stream. This is equivalent to calling [`shutdown(Write)`] on the `UnixStream`.
    ///
    /// [`split`]: Self::split()
    /// [`shutdown(Write)`]: fn@Self::shutdown
    pub fn into_split(self) -> (OwnedReadHalf, OwnedWriteHalf) {
        split_owned(self)
    }
}

impl TryFrom<net::UnixStream> for UnixStream {
    type Error = io::Error;

    /// Consumes stream, returning the tokio I/O object.
    ///
    /// This is equivalent to
    /// [`UnixStream::from_std(stream)`](UnixStream::from_std).
    fn try_from(stream: net::UnixStream) -> io::Result<Self> {
        Self::from_std(stream)
    }
}

impl AsyncRead for UnixStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll_read_priv(cx, buf)
    }
}

impl AsyncWrite for UnixStream {
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

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.shutdown(std::net::Shutdown::Write)?;
        Poll::Ready(Ok(()))
    }
}

impl UnixStream {
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
        // Safety: `UnixStream::read` correctly handles reads into uninitialized memory
        unsafe { self.io.poll_read(cx, buf) }
    }

    pub(crate) fn poll_write_priv(
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

impl fmt::Debug for UnixStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.fmt(f)
    }
}

impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}
