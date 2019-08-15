use crate::UnixStream;

use tokio_net::driver::Handle;
use tokio_net::util::PollEvented;

use futures_core::ready;
use futures_util::future::poll_fn;
use mio::Ready;
use mio_uds;
use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{self, SocketAddr};
use std::path::Path;
use std::task::{Context, Poll};

/// A Unix socket which can accept connections from other Unix sockets.
pub struct UnixListener {
    io: PollEvented<mio_uds::UnixListener>,
}

impl UnixListener {
    /// Creates a new `UnixListener` bound to the specified path.
    pub fn bind<P>(path: P) -> io::Result<UnixListener>
    where
        P: AsRef<Path>,
    {
        let listener = mio_uds::UnixListener::bind(path)?;
        let io = PollEvented::new(listener);
        Ok(UnixListener { io })
    }

    /// Consumes a `UnixListener` in the standard library and returns a
    /// nonblocking `UnixListener` from this crate.
    ///
    /// The returned listener will be associated with the given event loop
    /// specified by `handle` and is ready to perform I/O.
    pub fn from_std(listener: net::UnixListener, handle: &Handle) -> io::Result<UnixListener> {
        let listener = mio_uds::UnixListener::from_listener(listener)?;
        let io = PollEvented::new_with_handle(listener, handle)?;
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
        Ok((UnixStream::new(io), addr)).into()
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

    /// Consumes this listener, returning a stream of the sockets this listener
    /// accepts.
    ///
    /// This method returns an implementation of the `Stream` trait which
    /// resolves to the sockets the are accepted on this listener.
    #[cfg(feature = "async-traits")]
    pub fn incoming(self) -> crate::Incoming {
        crate::Incoming::new(self)
    }
}

impl TryFrom<UnixListener> for mio_uds::UnixListener {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`tokio_net::util::PollEvented::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    fn try_from(value: UnixListener) -> Result<Self, Self::Error> {
        value.io.into_inner()
    }
}

impl TryFrom<net::UnixListener> for UnixListener {
    type Error = io::Error;

    /// Consumes stream, returning the tokio I/O object.
    ///
    /// This is equivalent to
    /// [`UnixListener::from_std(stream, &Handle::default())`](UnixListener::from_std).
    fn try_from(stream: net::UnixListener) -> io::Result<Self> {
        Self::from_std(stream, &Handle::default())
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
