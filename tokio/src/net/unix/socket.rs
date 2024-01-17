use std::io;
use std::path::Path;

use std::os::unix::io::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, RawFd};

use crate::net::{UnixDatagram, UnixListener, UnixStream};

cfg_net_unix! {
    /// A Unix socket that has not yet been converted to a `UnixStream`, `UnixDatagram`, or
    /// `UnixListener`.
    ///
    /// `UnixSocket` wraps an operating system socket and enables the caller to
    /// configure the socket before establishing a connection or accepting
    /// inbound connections. The caller is able to set socket option and explicitly
    /// bind the socket with a socket address.
    ///
    /// The underlying socket is closed when the `UnixSocket` value is dropped.
    ///
    /// `UnixSocket` should only be used directly if the default configuration used
    /// by `UnixStream::connect`, `UnixDatagram::bind`, and `UnixListener::bind`
    /// does not meet the required use case.
    ///
    /// Calling `UnixStream::connect(path)` is equivalent to:
    ///
    /// ```no_run
    /// use tokio::net::UnixSocket;
    /// use std::error::Error;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let dir = tempfile::tempdir().unwrap();
    ///     let path = dir.path().join("bind_path");
    ///     let socket = UnixSocket::new_stream()?;
    ///
    ///     let stream = socket.connect(path).await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    ///
    /// Calling `UnixDatagram::bind(path)` is equivalent to:
    ///
    /// ```no_run
    /// use tokio::net::UnixSocket;
    /// use std::error::Error;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let dir = tempfile::tempdir().unwrap();
    ///     let path = dir.path().join("bind_path");
    ///     let socket = UnixSocket::new_datagram()?;
    ///     socket.bind(path)?;
    ///
    ///     let datagram = socket.datagram()?;
    ///
    ///     Ok(())
    /// }
    /// ```
    ///
    /// Calling `UnixListener::bind(path)` is equivalent to:
    ///
    /// ```no_run
    /// use tokio::net::UnixSocket;
    /// use std::error::Error;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let dir = tempfile::tempdir().unwrap();
    ///     let path = dir.path().join("bind_path");
    ///     let socket = UnixSocket::new_stream()?;
    ///     socket.bind(path)?;
    ///
    ///     let listener = socket.listen(1024)?;
    ///
    ///     Ok(())
    /// }
    /// ```
    ///
    /// Setting socket options not explicitly provided by `UnixSocket` may be done by
    /// accessing the `RawFd`/`RawSocket` using [`AsRawFd`]/[`AsRawSocket`] and
    /// setting the option with a crate like [`socket2`].
    ///
    /// [`RawFd`]: https://doc.rust-lang.org/std/os/unix/io/type.RawFd.html
    /// [`RawSocket`]: https://doc.rust-lang.org/std/os/windows/io/type.RawSocket.html
    /// [`AsRawFd`]: https://doc.rust-lang.org/std/os/unix/io/trait.AsRawFd.html
    /// [`AsRawSocket`]: https://doc.rust-lang.org/std/os/windows/io/trait.AsRawSocket.html
    /// [`socket2`]: https://docs.rs/socket2/
    #[derive(Debug)]
    pub struct UnixSocket {
        inner: socket2::Socket,
    }
}

impl UnixSocket {
    fn ty(&self) -> socket2::Type {
        self.inner.r#type().unwrap()
    }

    /// Creates a new Unix datagram socket.
    ///
    /// Calls `socket(2)` with `AF_UNIX` and `SOCK_DGRAM`.
    ///
    /// # Returns
    ///
    /// On success, the newly created `UnixSocket` is returned. If an error is
    /// encountered, it is returned instead.
    pub fn new_datagram() -> io::Result<UnixSocket> {
        UnixSocket::new(socket2::Type::DGRAM)
    }

    /// Creates a new Unix stream socket.
    ///
    /// Calls `socket(2)` with `AF_UNIX` and `SOCK_STREAM`.
    ///
    /// # Returns
    ///
    /// On success, the newly created `UnixSocket` is returned. If an error is
    /// encountered, it is returned instead.
    pub fn new_stream() -> io::Result<UnixSocket> {
        UnixSocket::new(socket2::Type::STREAM)
    }

    fn new(ty: socket2::Type) -> io::Result<UnixSocket> {
        #[cfg(any(
            target_os = "android",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "fuchsia",
            target_os = "illumos",
            target_os = "linux",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        let ty = ty.nonblocking();
        let inner = socket2::Socket::new(socket2::Domain::UNIX, ty, None)?;
        #[cfg(not(any(
            target_os = "android",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "fuchsia",
            target_os = "illumos",
            target_os = "linux",
            target_os = "netbsd",
            target_os = "openbsd"
        )))]
        inner.set_nonblocking(true)?;
        Ok(UnixSocket { inner })
    }

    /// Binds the socket to the given address.
    ///
    /// This calls the `bind(2)` operating-system function.
    pub fn bind(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let addr = socket2::SockAddr::unix(path)?;
        self.inner.bind(&addr)
    }

    /// Converts the socket into a `UnixListener`.
    ///
    /// `backlog` defines the maximum number of pending connections are queued
    /// by the operating system at any given time. Connection are removed from
    /// the queue with [`UnixListener::accept`]. When the queue is full, the
    /// operating-system will start rejecting connections.
    ///
    /// Calling this function on a socket created by `new_datagram` will return an error.
    ///
    /// [`UnixListener::accept`]: UnixListener::accept
    ///
    /// This calls the `listen(2)` operating-system function, marking the socket
    /// as a passive socket.
    pub fn listen(self, backlog: u32) -> io::Result<UnixListener> {
        if self.ty() == socket2::Type::DGRAM {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "listen cannot be called on a datagram socket",
            ));
        }

        self.inner.listen(backlog as i32)?;
        let mio = {
            use std::os::unix::io::{FromRawFd, IntoRawFd};

            let raw_fd = self.inner.into_raw_fd();
            unsafe { mio::net::UnixListener::from_raw_fd(raw_fd) }
        };

        UnixListener::new(mio)
    }

    /// Establishes a Unix connection with a peer at the specified socket address.
    ///
    /// The `UnixSocket` is consumed. Once the connection is established, a
    /// connected [`UnixStream`] is returned. If the connection fails, the
    /// encountered error is returned. Also, calling this function on a socket
    /// created by `new_datagram` will return an error.
    ///
    /// [`UnixStream`]: UnixStream
    ///
    /// This calls the `connect(2)` operating-system function.
    pub async fn connect(self, path: impl AsRef<Path>) -> io::Result<UnixStream> {
        if self.ty() == socket2::Type::DGRAM {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "connect cannot be called on a datagram socket",
            ));
        }

        let addr = socket2::SockAddr::unix(path)?;
        if let Err(err) = self.inner.connect(&addr) {
            if err.raw_os_error() != Some(libc::EINPROGRESS) {
                return Err(err);
            }
        }
        let mio = {
            use std::os::unix::io::{FromRawFd, IntoRawFd};

            let raw_fd = self.inner.into_raw_fd();
            unsafe { mio::net::UnixStream::from_raw_fd(raw_fd) }
        };

        UnixStream::connect_mio(mio).await
    }

    /// Converts the socket into a `UnixDatagram`.
    pub fn datagram(self) -> io::Result<UnixDatagram> {
        if self.ty() == socket2::Type::STREAM {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "datagram cannot be called on a stream socket",
            ));
        }
        let mio = {
            use std::os::unix::io::{FromRawFd, IntoRawFd};

            let raw_fd = self.inner.into_raw_fd();
            unsafe { mio::net::UnixDatagram::from_raw_fd(raw_fd) }
        };

        UnixDatagram::from_mio(mio)
    }
}

#[cfg(unix)]
impl AsRawFd for UnixSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

#[cfg(unix)]
impl AsFd for UnixSocket {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.as_raw_fd()) }
    }
}

#[cfg(unix)]
impl FromRawFd for UnixSocket {
    /// Converts a `RawFd` to a `UnixSocket`.
    ///
    /// # Notes
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_fd(fd: RawFd) -> UnixSocket {
        let inner = socket2::Socket::from_raw_fd(fd);
        UnixSocket { inner }
    }
}

#[cfg(unix)]
impl IntoRawFd for UnixSocket {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}
