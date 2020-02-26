use crate::future::poll_fn;
use crate::io::PollEvented;
use crate::net::unix::{Incoming, UnixStream};

use mio::Ready;
use mio_uds;
use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{self, SocketAddr};
use std::path::Path;
use std::task::{Context, Poll};

cfg_uds! {
    /// A Unix socket which can accept connections from other Unix sockets.
    ///
    /// You can accept a new connection by using the [`accept`](`UnixListener::accept`) method. Alternatively `UnixListener`
    /// implements the [`Stream`](`crate::stream::Stream`) trait, which allows you to use the listener in places that want a
    /// stream. The stream will never return `None` and will also not yield the peer's `SocketAddr` structure. Iterating over
    /// it is equivalent to calling accept in a loop.
    ///
    /// # Errors
    ///
    /// Note that accepting a connection can lead to various errors and not all
    /// of them are necessarily fatal ‒ for example having too many open file
    /// descriptors or the other side closing the connection while it waits in
    /// an accept queue. These would terminate the stream if not handled in any
    /// way.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::UnixListener;
    /// use tokio::stream::StreamExt;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut listener = UnixListener::bind("/path/to/the/socket").unwrap();
    ///     while let Some(stream) = listener.next().await {
    ///         match stream {
    ///             Ok(stream) => {
    ///                 println!("new client!");
    ///             }
    ///             Err(e) => { /* connection failed */ }
    ///         }
    ///     }
    /// }
    /// ```
    pub struct UnixListener {
        io: PollEvented<mio_uds::UnixListener>,
    }
}

impl UnixListener {
    /// Creates a new `UnixListener` bound to the specified path.
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Handle::enter`](crate::runtime::Handle::enter) function.
    pub fn bind<P>(path: P) -> io::Result<UnixListener>
    where
        P: AsRef<Path>,
    {
        let listener = mio_uds::UnixListener::bind(path)?;
        let io = PollEvented::new(listener)?;
        Ok(UnixListener { io })
    }

    /// Consumes a `UnixListener` in the standard library and returns a
    /// nonblocking `UnixListener` from this crate.
    ///
    /// The returned listener will be associated with the given event loop
    /// specified by `handle` and is ready to perform I/O.
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Handle::enter`](crate::runtime::Handle::enter) function.
    pub fn from_std(listener: net::UnixListener) -> io::Result<UnixListener> {
        let listener = mio_uds::UnixListener::from_listener(listener)?;
        let io = PollEvented::new(listener)?;
        Ok(UnixListener { io })
    }

    /// Returns the local socket address of this listener.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.get_ref().take_error()
    }

    /// Accepts a new incoming connection to this listener.
    pub async fn accept(&mut self) -> io::Result<(UnixStream, SocketAddr)> {
        poll_fn(|cx| self.poll_accept(cx)).await
    }

    pub(crate) fn poll_accept(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<(UnixStream, SocketAddr)>> {
        let (io, addr) = ready!(self.poll_accept_std(cx))?;

        let io = mio_uds::UnixStream::from_stream(io)?;
        Ok((UnixStream::new(io)?, addr)).into()
    }

    fn poll_accept_std(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<(net::UnixStream, SocketAddr)>> {
        ready!(self.io.poll_read_ready(cx, Ready::readable()))?;

        match self.io.get_ref().accept_std() {
            Ok(None) => {
                self.io.clear_read_ready(cx, Ready::readable())?;
                Poll::Pending
            }
            Ok(Some((sock, addr))) => Ok((sock, addr)).into(),
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, Ready::readable())?;
                Poll::Pending
            }
            Err(err) => Err(err).into(),
        }
    }

    /// Returns a stream over the connections being received on this listener.
    ///
    /// Note that `UnixListener` also directly implements `Stream`.
    ///
    /// The returned stream will never return `None` and will also not yield the
    /// peer's `SocketAddr` structure. Iterating over it is equivalent to
    /// calling accept in a loop.
    ///
    /// # Errors
    ///
    /// Note that accepting a connection can lead to various errors and not all
    /// of them are necessarily fatal ‒ for example having too many open file
    /// descriptors or the other side closing the connection while it waits in
    /// an accept queue. These would terminate the stream if not handled in any
    /// way.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::UnixListener;
    /// use tokio::stream::StreamExt;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut listener = UnixListener::bind("/path/to/the/socket").unwrap();
    ///     let mut incoming = listener.incoming();
    ///
    ///     while let Some(stream) = incoming.next().await {
    ///         match stream {
    ///             Ok(stream) => {
    ///                 println!("new client!");
    ///             }
    ///             Err(e) => { /* connection failed */ }
    ///         }
    ///     }
    /// }
    /// ```
    pub fn incoming(&mut self) -> Incoming<'_> {
        Incoming::new(self)
    }
}

#[cfg(feature = "stream")]
impl crate::stream::Stream for UnixListener {
    type Item = io::Result<UnixStream>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let (socket, _) = ready!(self.poll_accept(cx))?;
        Poll::Ready(Some(Ok(socket)))
    }
}

impl TryFrom<UnixListener> for mio_uds::UnixListener {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`PollEvented::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    ///
    /// [`PollEvented::into_inner`]: crate::io::PollEvented::into_inner
    fn try_from(value: UnixListener) -> Result<Self, Self::Error> {
        value.io.into_inner()
    }
}

impl TryFrom<net::UnixListener> for UnixListener {
    type Error = io::Error;

    /// Consumes stream, returning the tokio I/O object.
    ///
    /// This is equivalent to
    /// [`UnixListener::from_std(stream)`](UnixListener::from_std).
    fn try_from(stream: net::UnixListener) -> io::Result<Self> {
        Self::from_std(stream)
    }
}

impl fmt::Debug for UnixListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        self.io.get_ref().as_raw_fd()
    }
}
