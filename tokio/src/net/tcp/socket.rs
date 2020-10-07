use crate::net::{TcpListener, TcpStream};

use std::fmt;
use std::io;
use std::net::SocketAddr;

#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};

/// A TCP socket that has not yet been converted to a `TcpStream` or
/// `TcpListener`.
///
/// `TcpSocket` wraps an operating system socket and enables the caller to
/// configure the socket before establishing a TCP connection or accepting
/// inbound connections. The caller is able to set socket option and explicitly
/// bind the socket with a socket address.
///
/// The underlying socket is closed when the `TcpSocket` value is dropped.
///
/// `TcpSocket` should only be used directly if the default configuration used
/// by `TcpStream::connect` and `TcpListener::bind` does not meet the required
/// use case.
///
/// Calling `TcpStream::connect("127.0.0.1:8080")` is equivalent to:
///
/// ```no_run
/// use tokio::net::TcpSocket;
///
/// use std::io;
///
/// #[tokio::main]
/// async fn main() -> io::Result<()> {
///     let addr = "127.0.0.1:8080".parse().unwrap();
///
///     let socket = TcpSocket::new_v4()?;
///     let stream = socket.connect(addr).await?;
/// # drop(stream);
///
///     Ok(())
/// }
/// ```
///
/// Calling `TcpListener::bind("127.0.0.1:8080")` is equivalent to:
///
/// ```no_run
/// use tokio::net::TcpSocket;
///
/// use std::io;
///
/// #[tokio::main]
/// async fn main() -> io::Result<()> {
///     let addr = "127.0.0.1:8080".parse().unwrap();
///
///     let socket = TcpSocket::new_v4()?;
///     // On platforms with Berkeley-derived sockets, this allows to quickly
///     // rebind a socket, without needing to wait for the OS to clean up the
///     // previous one.
///     //
///     // On Windows, this allows rebinding sockets which are actively in use,
///     // which allows “socket hijacking”, so we explicitly don't set it here.
///     // https://docs.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse
///     socket.set_reuseaddr(true)?;
///     socket.bind(addr)?;
///
///     let listener = socket.listen(1024)?;
/// # drop(listener);
///
///     Ok(())
/// }
/// ```
///
/// Setting socket options not explicitly provided by `TcpSocket` may be done by
/// accessing the `RawFd`/`RawSocket` using [`AsRawFd`]/[`AsRawSocket`] and
/// setting the option with a crate like [`socket2`].
///
/// [`RawFd`]: https://doc.rust-lang.org/std/os/unix/io/type.RawFd.html
/// [`RawSocket`]: https://doc.rust-lang.org/std/os/windows/io/type.RawSocket.html
/// [`AsRawFd`]: https://doc.rust-lang.org/std/os/unix/io/trait.AsRawFd.html
/// [`AsRawSocket`]: https://doc.rust-lang.org/std/os/windows/io/trait.AsRawSocket.html
/// [`socket2`]: https://docs.rs/socket2/
pub struct TcpSocket {
    inner: mio::net::TcpSocket,
}

impl TcpSocket {
    /// Create a new socket configured for IPv4.
    ///
    /// Calls `socket(2)` with `AF_INET` and `SOCK_STREAM`.
    ///
    /// # Returns
    ///
    /// On success, the newly created `TcpSocket` is returned. If an error is
    /// encountered, it is returned instead.
    ///
    /// # Examples
    ///
    /// Create a new IPv4 socket and start listening.
    ///
    /// ```no_run
    /// use tokio::net::TcpSocket;
    ///
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let addr = "127.0.0.1:8080".parse().unwrap();
    ///     let socket = TcpSocket::new_v4()?;
    ///     socket.bind(addr)?;
    ///
    ///     let listener = socket.listen(128)?;
    /// # drop(listener);
    ///     Ok(())
    /// }
    /// ```
    pub fn new_v4() -> io::Result<TcpSocket> {
        let inner = mio::net::TcpSocket::new_v4()?;
        Ok(TcpSocket { inner })
    }

    /// Create a new socket configured for IPv6.
    ///
    /// Calls `socket(2)` with `AF_INET6` and `SOCK_STREAM`.
    ///
    /// # Returns
    ///
    /// On success, the newly created `TcpSocket` is returned. If an error is
    /// encountered, it is returned instead.
    ///
    /// # Examples
    ///
    /// Create a new IPv6 socket and start listening.
    ///
    /// ```no_run
    /// use tokio::net::TcpSocket;
    ///
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let addr = "[::1]:8080".parse().unwrap();
    ///     let socket = TcpSocket::new_v6()?;
    ///     socket.bind(addr)?;
    ///
    ///     let listener = socket.listen(128)?;
    /// # drop(listener);
    ///     Ok(())
    /// }
    /// ```
    pub fn new_v6() -> io::Result<TcpSocket> {
        let inner = mio::net::TcpSocket::new_v6()?;
        Ok(TcpSocket { inner })
    }

    /// Allow the socket to bind to an in-use address.
    ///
    /// Behavior is platform specific. Refer to the target platform's
    /// documentation for more details.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpSocket;
    ///
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let addr = "127.0.0.1:8080".parse().unwrap();
    ///
    ///     let socket = TcpSocket::new_v4()?;
    ///     socket.set_reuseaddr(true)?;
    ///     socket.bind(addr)?;
    ///
    ///     let listener = socket.listen(1024)?;
    /// # drop(listener);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn set_reuseaddr(&self, reuseaddr: bool) -> io::Result<()> {
        self.inner.set_reuseaddr(reuseaddr)
    }

    /// Bind the socket to the given address.
    ///
    /// This calls the `bind(2)` operating-system function. Behavior is
    /// platform specific. Refer to the target platform's documentation for more
    /// details.
    ///
    /// # Examples
    ///
    /// Bind a socket before listening.
    ///
    /// ```no_run
    /// use tokio::net::TcpSocket;
    ///
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let addr = "127.0.0.1:8080".parse().unwrap();
    ///
    ///     let socket = TcpSocket::new_v4()?;
    ///     socket.bind(addr)?;
    ///
    ///     let listener = socket.listen(1024)?;
    /// # drop(listener);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn bind(&self, addr: SocketAddr) -> io::Result<()> {
        self.inner.bind(addr)
    }

    /// Establish a TCP connection with a peer at the specified socket address.
    ///
    /// The `TcpSocket` is consumed. Once the connection is established, a
    /// connected [`TcpStream`] is returned. If the connection fails, the
    /// encountered error is returned.
    ///
    /// [`TcpStream`]: TcpStream
    ///
    /// This calls the `connect(2)` operating-system function. Behavior is
    /// platform specific. Refer to the target platform's documentation for more
    /// details.
    ///
    /// # Examples
    ///
    /// Connecting to a peer.
    ///
    /// ```no_run
    /// use tokio::net::TcpSocket;
    ///
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let addr = "127.0.0.1:8080".parse().unwrap();
    ///
    ///     let socket = TcpSocket::new_v4()?;
    ///     let stream = socket.connect(addr).await?;
    /// # drop(stream);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect(self, addr: SocketAddr) -> io::Result<TcpStream> {
        let mio = self.inner.connect(addr)?;
        TcpStream::connect_mio(mio).await
    }

    /// Convert the socket into a `TcpListener`.
    ///
    /// `backlog` defines the maximum number of pending connections are queued
    /// by the operating system at any given time. Connection are removed from
    /// the queue with [`TcpListener::accept`]. When the queue is full, the
    /// operationg-system will start rejecting connections.
    ///
    /// [`TcpListener::accept`]: TcpListener::accept
    ///
    /// This calls the `listen(2)` operating-system function, marking the socket
    /// as a passive socket. Behavior is platform specific. Refer to the target
    /// platform's documentation for more details.
    ///
    /// # Examples
    ///
    /// Create a `TcpListener`.
    ///
    /// ```no_run
    /// use tokio::net::TcpSocket;
    ///
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let addr = "127.0.0.1:8080".parse().unwrap();
    ///
    ///     let socket = TcpSocket::new_v4()?;
    ///     socket.bind(addr)?;
    ///
    ///     let listener = socket.listen(1024)?;
    /// # drop(listener);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn listen(self, backlog: u32) -> io::Result<TcpListener> {
        let mio = self.inner.listen(backlog)?;
        TcpListener::new(mio)
    }
}

impl fmt::Debug for TcpSocket {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(fmt)
    }
}

#[cfg(unix)]
impl AsRawFd for TcpSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

#[cfg(unix)]
impl FromRawFd for TcpSocket {
    /// Converts a `RawFd` to a `TcpSocket`.
    ///
    /// # Notes
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_fd(fd: RawFd) -> TcpSocket {
        let inner = mio::net::TcpSocket::from_raw_fd(fd);
        TcpSocket { inner }
    }
}

#[cfg(windows)]
impl IntoRawSocket for TcpSocket {
    fn into_raw_socket(self) -> RawSocket {
        self.inner.into_raw_socket()
    }
}

#[cfg(windows)]
impl AsRawSocket for TcpSocket {
    fn as_raw_socket(&self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}

#[cfg(windows)]
impl FromRawSocket for TcpSocket {
    /// Converts a `RawSocket` to a `TcpStream`.
    ///
    /// # Notes
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_socket(socket: RawSocket) -> TcpSocket {
        let inner = mio::net::TcpSocket::from_raw_socket(socket);
        TcpSocket { inner }
    }
}
