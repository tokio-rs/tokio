use crate::future::poll_fn;
use crate::io::IoResource;
use crate::net::unix::{Incoming, UnixStream};

use mio;
use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net;
use std::path::Path;
use std::task::{Context, Poll};

cfg_uds! {
    /// A Unix socket which can accept connections from other Unix sockets.
    pub struct UnixListener {
        io: IoResource<mio::net::UnixListener>,
    }
}

impl UnixListener {
    /// Creates a new `UnixListener` bound to the specified path.
    pub fn bind<P>(path: P) -> io::Result<UnixListener>
    where
        P: AsRef<Path>,
    {
        let listener = mio::net::UnixListener::bind(path)?;
        let io = IoResource::new(listener)?;
        Ok(UnixListener { io })
    }

    /// Consumes a `UnixListener` in the standard library and returns a
    /// nonblocking `UnixListener` from this crate.
    ///
    /// The returned listener will be associated with the given event loop
    /// specified by `handle` and is ready to perform I/O.
    pub fn from_std(listener: net::UnixListener) -> io::Result<UnixListener> {
        let listener = mio::net::UnixListener::from_std(listener);
        let io = IoResource::new(listener)?;
        Ok(UnixListener { io })
    }

    /// Returns the local socket address of this listener.
    pub fn local_addr(&self) -> io::Result<mio::unix::SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.get_ref().take_error()
    }

    /// Accepts a new incoming connection to this listener.
    pub async fn accept(&mut self) -> io::Result<(UnixStream, mio::unix::SocketAddr)> {
        poll_fn(|cx| self.poll_accept(cx)).await
    }

    pub(crate) fn poll_accept(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<(UnixStream, mio::unix::SocketAddr)>> {
        ready!(self.io.poll_read_ready(cx))?;

        match self.io.get_ref().accept() {
            Ok((stream, sockaddr)) => {
                let stream = UnixStream::new(stream)?;
                Ok((stream, sockaddr)).into()
            }
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx)?;
                Poll::Pending
            }
            Err(err) => Err(err).into(),
        }
    }

    /// Returns a stream over the connections being received on this listener.
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
    ///
    /// use futures::StreamExt;
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

impl TryFrom<UnixListener> for mio::net::UnixListener {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`IoResource::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    ///
    /// [`IoResource::into_inner`]: crate::io::IoResource::into_inner
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
