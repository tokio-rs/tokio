use {Incoming, UnixStream};

use tokio_reactor::{Handle, PollEvented};

use futures::{Async, Poll};
use mio::Ready;
use mio_uds;

use std::fmt;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{self, SocketAddr};
use std::path::Path;

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

    /// Test whether this socket is ready to be read or not.
    pub fn poll_read_ready(&self, ready: Ready) -> Poll<Ready, io::Error> {
        self.io.poll_read_ready(ready)
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.get_ref().take_error()
    }

    /// Attempt to accept a connection and create a new connected `UnixStream`
    /// if successful.
    ///
    /// This function will attempt an accept operation, but will not block
    /// waiting for it to complete. If the operation would block then a "would
    /// block" error is returned. Additionally, if this method would block, it
    /// registers the current task to receive a notification when it would
    /// otherwise not block.
    ///
    /// Note that typically for simple usage it's easier to treat incoming
    /// connections as a `Stream` of `UnixStream`s with the `incoming` method
    /// below.
    ///
    /// # Panics
    ///
    /// This function will panic if it is called outside the context of a
    /// future's task. It's recommended to only call this from the
    /// implementation of a `Future::poll`, if necessary.
    pub fn poll_accept(&self) -> Poll<(UnixStream, SocketAddr), io::Error> {
        let (io, addr) = try_ready!(self.poll_accept_std());

        let io = mio_uds::UnixStream::from_stream(io)?;
        Ok((UnixStream::new(io), addr).into())
    }

    /// Attempt to accept a connection and create a new connected `UnixStream`
    /// if successful.
    ///
    /// This function is the same as `poll_accept` above except that it returns a
    /// `mio_uds::UnixStream` instead of a `tokio_udp::UnixStream`. This in turn
    /// can then allow for the stream to be associated with a different reactor
    /// than the one this `UnixListener` is associated with.
    ///
    /// This function will attempt an accept operation, but will not block
    /// waiting for it to complete. If the operation would block then a "would
    /// block" error is returned. Additionally, if this method would block, it
    /// registers the current task to receive a notification when it would
    /// otherwise not block.
    ///
    /// Note that typically for simple usage it's easier to treat incoming
    /// connections as a `Stream` of `UnixStream`s with the `incoming` method
    /// below.
    ///
    /// # Panics
    ///
    /// This function will panic if it is called outside the context of a
    /// future's task. It's recommended to only call this from the
    /// implementation of a `Future::poll`, if necessary.
    pub fn poll_accept_std(&self) -> Poll<(net::UnixStream, SocketAddr), io::Error> {
        loop {
            try_ready!(self.io.poll_read_ready(Ready::readable()));

            match self.io.get_ref().accept_std() {
                Ok(None) => {
                    self.io.clear_read_ready(Ready::readable())?;
                    return Ok(Async::NotReady);
                }
                Ok(Some((sock, addr))) => {
                    return Ok(Async::Ready((sock, addr)));
                }
                Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                    self.io.clear_read_ready(Ready::readable())?;
                    return Ok(Async::NotReady);
                }
                Err(err) => return Err(err),
            }
        }
    }

    /// Consumes this listener, returning a stream of the sockets this listener
    /// accepts.
    ///
    /// This method returns an implementation of the `Stream` trait which
    /// resolves to the sockets the are accepted on this listener.
    pub fn incoming(self) -> Incoming {
        Incoming::new(self)
    }
}

impl fmt::Debug for UnixListener {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        self.io.get_ref().as_raw_fd()
    }
}
