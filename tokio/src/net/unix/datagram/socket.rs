use crate::io::{Interest, PollEvented};
use crate::net::unix::SocketAddr;

use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net;
use std::path::Path;

cfg_net_unix! {
    /// An I/O object representing a Unix datagram socket.
    ///
    /// A socket can be either named (associated with a filesystem path) or
    /// unnamed.
    ///
    /// **Note:** named sockets are persisted even after the object is dropped
    /// and the program has exited, and cannot be reconnected. It is advised
    /// that you either check for and unlink the existing socket if it exists,
    /// or use a temporary file that is guaranteed to not already exist.
    ///
    /// # Examples
    /// Using named sockets, associated with a filesystem path:
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    /// use tempfile::tempdir;
    ///
    /// // We use a temporary directory so that the socket
    /// // files left by the bound sockets will get cleaned up.
    /// let tmp = tempdir()?;
    ///
    /// // Bind each socket to a filesystem path
    /// let tx_path = tmp.path().join("tx");
    /// let tx = UnixDatagram::bind(&tx_path)?;
    /// let rx_path = tmp.path().join("rx");
    /// let rx = UnixDatagram::bind(&rx_path)?;
    ///
    /// let bytes = b"hello world";
    /// tx.send_to(bytes, &rx_path).await?;
    ///
    /// let mut buf = vec![0u8; 24];
    /// let (size, addr) = rx.recv_from(&mut buf).await?;
    ///
    /// let dgram = &buf[..size];
    /// assert_eq!(dgram, bytes);
    /// assert_eq!(addr.as_pathname().unwrap(), &tx_path);
    ///
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Using unnamed sockets, created as a pair
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    ///
    /// // Create the pair of sockets
    /// let (sock1, sock2) = UnixDatagram::pair()?;
    ///
    /// // Since the sockets are paired, the paired send/recv
    /// // functions can be used
    /// let bytes = b"hello world";
    /// sock1.send(bytes).await?;
    ///
    /// let mut buff = vec![0u8; 24];
    /// let size = sock2.recv(&mut buff).await?;
    ///
    /// let dgram = &buff[..size];
    /// assert_eq!(dgram, bytes);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub struct UnixDatagram {
        io: PollEvented<mio::net::UnixDatagram>,
    }
}

impl UnixDatagram {
    /// Creates a new `UnixDatagram` bound to the specified path.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    /// use tempfile::tempdir;
    ///
    /// // We use a temporary directory so that the socket
    /// // files left by the bound sockets will get cleaned up.
    /// let tmp = tempdir()?;
    ///
    /// // Bind the socket to a filesystem path
    /// let socket_path = tmp.path().join("socket");
    /// let socket = UnixDatagram::bind(&socket_path)?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn bind<P>(path: P) -> io::Result<UnixDatagram>
    where
        P: AsRef<Path>,
    {
        let socket = mio::net::UnixDatagram::bind(path)?;
        UnixDatagram::new(socket)
    }

    /// Creates an unnamed pair of connected sockets.
    ///
    /// This function will create a pair of interconnected Unix sockets for
    /// communicating back and forth between one another.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    ///
    /// // Create the pair of sockets
    /// let (sock1, sock2) = UnixDatagram::pair()?;
    ///
    /// // Since the sockets are paired, the paired send/recv
    /// // functions can be used
    /// let bytes = b"hail eris";
    /// sock1.send(bytes).await?;
    ///
    /// let mut buff = vec![0u8; 24];
    /// let size = sock2.recv(&mut buff).await?;
    ///
    /// let dgram = &buff[..size];
    /// assert_eq!(dgram, bytes);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn pair() -> io::Result<(UnixDatagram, UnixDatagram)> {
        let (a, b) = mio::net::UnixDatagram::pair()?;
        let a = UnixDatagram::new(a)?;
        let b = UnixDatagram::new(b)?;

        Ok((a, b))
    }

    /// Creates new `UnixDatagram` from a `std::os::unix::net::UnixDatagram`.
    ///
    /// This function is intended to be used to wrap a UnixDatagram from the
    /// standard library in the Tokio equivalent. The conversion assumes
    /// nothing about the underlying datagram; it is left up to the user to set
    /// it in non-blocking mode.
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a Tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    /// use std::os::unix::net::UnixDatagram as StdUDS;
    /// use tempfile::tempdir;
    ///
    /// // We use a temporary directory so that the socket
    /// // files left by the bound sockets will get cleaned up.
    /// let tmp = tempdir()?;
    ///
    /// // Bind the socket to a filesystem path
    /// let socket_path = tmp.path().join("socket");
    /// let std_socket = StdUDS::bind(&socket_path)?;
    /// std_socket.set_nonblocking(true)?;
    /// let tokio_socket = UnixDatagram::from_std(std_socket)?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_std(datagram: net::UnixDatagram) -> io::Result<UnixDatagram> {
        let socket = mio::net::UnixDatagram::from_std(datagram);
        let io = PollEvented::new(socket)?;
        Ok(UnixDatagram { io })
    }

    fn new(socket: mio::net::UnixDatagram) -> io::Result<UnixDatagram> {
        let io = PollEvented::new(socket)?;
        Ok(UnixDatagram { io })
    }

    /// Creates a new `UnixDatagram` which is not bound to any address.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    /// use tempfile::tempdir;
    ///
    /// // Create an unbound socket
    /// let tx = UnixDatagram::unbound()?;
    ///
    /// // Create another, bound socket
    /// let tmp = tempdir()?;
    /// let rx_path = tmp.path().join("rx");
    /// let rx = UnixDatagram::bind(&rx_path)?;
    ///
    /// // Send to the bound socket
    /// let bytes = b"hello world";
    /// tx.send_to(bytes, &rx_path).await?;
    ///
    /// let mut buf = vec![0u8; 24];
    /// let (size, addr) = rx.recv_from(&mut buf).await?;
    ///
    /// let dgram = &buf[..size];
    /// assert_eq!(dgram, bytes);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn unbound() -> io::Result<UnixDatagram> {
        let socket = mio::net::UnixDatagram::unbound()?;
        UnixDatagram::new(socket)
    }

    /// Connects the socket to the specified address.
    ///
    /// The `send` method may be used to send data to the specified address.
    /// `recv` and `recv_from` will only receive data from that address.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    /// use tempfile::tempdir;
    ///
    /// // Create an unbound socket
    /// let tx = UnixDatagram::unbound()?;
    ///
    /// // Create another, bound socket
    /// let tmp = tempdir()?;
    /// let rx_path = tmp.path().join("rx");
    /// let rx = UnixDatagram::bind(&rx_path)?;
    ///
    /// // Connect to the bound socket
    /// tx.connect(&rx_path)?;
    ///
    /// // Send to the bound socket
    /// let bytes = b"hello world";
    /// tx.send(bytes).await?;
    ///
    /// let mut buf = vec![0u8; 24];
    /// let (size, addr) = rx.recv_from(&mut buf).await?;
    ///
    /// let dgram = &buf[..size];
    /// assert_eq!(dgram, bytes);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn connect<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        self.io.connect(path)
    }

    /// Sends data on the socket to the socket's peer.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    ///
    /// // Create the pair of sockets
    /// let (sock1, sock2) = UnixDatagram::pair()?;
    ///
    /// // Since the sockets are paired, the paired send/recv
    /// // functions can be used
    /// let bytes = b"hello world";
    /// sock1.send(bytes).await?;
    ///
    /// let mut buff = vec![0u8; 24];
    /// let size = sock2.recv(&mut buff).await?;
    ///
    /// let dgram = &buff[..size];
    /// assert_eq!(dgram, bytes);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.io
            .registration()
            .async_io(Interest::WRITABLE, || self.io.send(buf))
            .await
    }

    /// Try to send a datagram to the peer without waiting.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use tokio::net::UnixDatagram;
    ///
    /// let bytes = b"bytes";
    /// // We use a socket pair so that they are assigned
    /// // each other as a peer.
    /// let (first, second) = UnixDatagram::pair()?;
    ///
    /// let size = first.try_send(bytes)?;
    /// assert_eq!(size, bytes.len());
    ///
    /// let mut buffer = vec![0u8; 24];
    /// let size = second.try_recv(&mut buffer)?;
    ///
    /// let dgram = &buffer[..size];
    /// assert_eq!(dgram, bytes);
    /// # Ok(())
    /// # }
    /// ```
    pub fn try_send(&self, buf: &[u8]) -> io::Result<usize> {
        self.io.send(buf)
    }

    /// Try to send a datagram to the peer without waiting.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use tokio::net::UnixDatagram;
    /// use tempfile::tempdir;
    ///
    /// let bytes = b"bytes";
    /// // We use a temporary directory so that the socket
    /// // files left by the bound sockets will get cleaned up.
    /// let tmp = tempdir().unwrap();
    ///
    /// let server_path = tmp.path().join("server");
    /// let server = UnixDatagram::bind(&server_path)?;
    ///
    /// let client_path = tmp.path().join("client");
    /// let client = UnixDatagram::bind(&client_path)?;
    ///
    /// let size = client.try_send_to(bytes, &server_path)?;
    /// assert_eq!(size, bytes.len());
    ///
    /// let mut buffer = vec![0u8; 24];
    /// let (size, addr) = server.try_recv_from(&mut buffer)?;
    ///
    /// let dgram = &buffer[..size];
    /// assert_eq!(dgram, bytes);
    /// assert_eq!(addr.as_pathname().unwrap(), &client_path);
    /// # Ok(())
    /// # }
    /// ```
    pub fn try_send_to<P>(&self, buf: &[u8], target: P) -> io::Result<usize>
    where
        P: AsRef<Path>,
    {
        self.io.send_to(buf, target)
    }

    /// Receives data from the socket.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    ///
    /// // Create the pair of sockets
    /// let (sock1, sock2) = UnixDatagram::pair()?;
    ///
    /// // Since the sockets are paired, the paired send/recv
    /// // functions can be used
    /// let bytes = b"hello world";
    /// sock1.send(bytes).await?;
    ///
    /// let mut buff = vec![0u8; 24];
    /// let size = sock2.recv(&mut buff).await?;
    ///
    /// let dgram = &buff[..size];
    /// assert_eq!(dgram, bytes);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.io
            .registration()
            .async_io(Interest::READABLE, || self.io.recv(buf))
            .await
    }

    /// Try to receive a datagram from the peer without waiting.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use tokio::net::UnixDatagram;
    ///
    /// let bytes = b"bytes";
    /// // We use a socket pair so that they are assigned
    /// // each other as a peer.
    /// let (first, second) = UnixDatagram::pair()?;
    ///
    /// let size = first.try_send(bytes)?;
    /// assert_eq!(size, bytes.len());
    ///
    /// let mut buffer = vec![0u8; 24];
    /// let size = second.try_recv(&mut buffer)?;
    ///
    /// let dgram = &buffer[..size];
    /// assert_eq!(dgram, bytes);
    /// # Ok(())
    /// # }
    /// ```
    pub fn try_recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.recv(buf)
    }

    /// Sends data on the socket to the specified address.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    /// use tempfile::tempdir;
    ///
    /// // We use a temporary directory so that the socket
    /// // files left by the bound sockets will get cleaned up.
    /// let tmp = tempdir()?;
    ///
    /// // Bind each socket to a filesystem path
    /// let tx_path = tmp.path().join("tx");
    /// let tx = UnixDatagram::bind(&tx_path)?;
    /// let rx_path = tmp.path().join("rx");
    /// let rx = UnixDatagram::bind(&rx_path)?;
    ///
    /// let bytes = b"hello world";
    /// tx.send_to(bytes, &rx_path).await?;
    ///
    /// let mut buf = vec![0u8; 24];
    /// let (size, addr) = rx.recv_from(&mut buf).await?;
    ///
    /// let dgram = &buf[..size];
    /// assert_eq!(dgram, bytes);
    /// assert_eq!(addr.as_pathname().unwrap(), &tx_path);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_to<P>(&self, buf: &[u8], target: P) -> io::Result<usize>
    where
        P: AsRef<Path>,
    {
        self.io
            .registration()
            .async_io(Interest::WRITABLE, || self.io.send_to(buf, target.as_ref()))
            .await
    }

    /// Receives data from the socket.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    /// use tempfile::tempdir;
    ///
    /// // We use a temporary directory so that the socket
    /// // files left by the bound sockets will get cleaned up.
    /// let tmp = tempdir()?;
    ///
    /// // Bind each socket to a filesystem path
    /// let tx_path = tmp.path().join("tx");
    /// let tx = UnixDatagram::bind(&tx_path)?;
    /// let rx_path = tmp.path().join("rx");
    /// let rx = UnixDatagram::bind(&rx_path)?;
    ///
    /// let bytes = b"hello world";
    /// tx.send_to(bytes, &rx_path).await?;
    ///
    /// let mut buf = vec![0u8; 24];
    /// let (size, addr) = rx.recv_from(&mut buf).await?;
    ///
    /// let dgram = &buf[..size];
    /// assert_eq!(dgram, bytes);
    /// assert_eq!(addr.as_pathname().unwrap(), &tx_path);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let (n, addr) = self
            .io
            .registration()
            .async_io(Interest::READABLE, || self.io.recv_from(buf))
            .await?;

        Ok((n, SocketAddr(addr)))
    }

    /// Try to receive data from the socket without waiting.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use tokio::net::UnixDatagram;
    /// use tempfile::tempdir;
    ///
    /// let bytes = b"bytes";
    /// // We use a temporary directory so that the socket
    /// // files left by the bound sockets will get cleaned up.
    /// let tmp = tempdir().unwrap();
    ///
    /// let server_path = tmp.path().join("server");
    /// let server = UnixDatagram::bind(&server_path)?;
    ///
    /// let client_path = tmp.path().join("client");
    /// let client = UnixDatagram::bind(&client_path)?;
    ///
    /// let size = client.try_send_to(bytes, &server_path)?;
    /// assert_eq!(size, bytes.len());
    ///
    /// let mut buffer = vec![0u8; 24];
    /// let (size, addr) = server.try_recv_from(&mut buffer)?;
    ///
    /// let dgram = &buffer[..size];
    /// assert_eq!(dgram, bytes);
    /// assert_eq!(addr.as_pathname().unwrap(), &client_path);
    /// # Ok(())
    /// # }
    /// ```
    pub fn try_recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let (n, addr) = self.io.recv_from(buf)?;
        Ok((n, SocketAddr(addr)))
    }

    /// Returns the local address that this socket is bound to.
    ///
    /// # Examples
    /// For a socket bound to a local path
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    /// use tempfile::tempdir;
    ///
    /// // We use a temporary directory so that the socket
    /// // files left by the bound sockets will get cleaned up.
    /// let tmp = tempdir()?;
    ///
    /// // Bind socket to a filesystem path
    /// let socket_path = tmp.path().join("socket");
    /// let socket = UnixDatagram::bind(&socket_path)?;
    ///
    /// assert_eq!(socket.local_addr()?.as_pathname().unwrap(), &socket_path);
    ///
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// For an unbound socket
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    ///
    /// // Create an unbound socket
    /// let socket = UnixDatagram::unbound()?;
    ///
    /// assert!(socket.local_addr()?.is_unnamed());
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr().map(SocketAddr)
    }

    /// Returns the address of this socket's peer.
    ///
    /// The `connect` method will connect the socket to a peer.
    ///
    /// # Examples
    /// For a peer with a local path
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    /// use tempfile::tempdir;
    ///
    /// // Create an unbound socket
    /// let tx = UnixDatagram::unbound()?;
    ///
    /// // Create another, bound socket
    /// let tmp = tempdir()?;
    /// let rx_path = tmp.path().join("rx");
    /// let rx = UnixDatagram::bind(&rx_path)?;
    ///
    /// // Connect to the bound socket
    /// tx.connect(&rx_path)?;
    ///
    /// assert_eq!(tx.peer_addr()?.as_pathname().unwrap(), &rx_path);
    ///
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// For an unbound peer
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    ///
    /// // Create the pair of sockets
    /// let (sock1, sock2) = UnixDatagram::pair()?;
    ///
    /// assert!(sock1.peer_addr()?.is_unnamed());
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.io.peer_addr().map(SocketAddr)
    }

    /// Returns the value of the `SO_ERROR` option.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    ///
    /// // Create an unbound socket
    /// let socket = UnixDatagram::unbound()?;
    ///
    /// if let Ok(Some(err)) = socket.take_error() {
    ///     println!("Got error: {:?}", err);
    /// }
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.take_error()
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O calls on the
    /// specified portions to immediately return with an appropriate value
    /// (see the documentation of `Shutdown`).
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    /// use std::net::Shutdown;
    ///
    /// // Create an unbound socket
    /// let (socket, other) = UnixDatagram::pair()?;
    ///
    /// socket.shutdown(Shutdown::Both)?;
    ///
    /// // NOTE: the following commented out code does NOT work as expected.
    /// // Due to an underlying issue, the recv call will block indefinitely.
    /// // See: https://github.com/tokio-rs/tokio/issues/1679
    /// //let mut buff = vec![0u8; 24];
    /// //let size = socket.recv(&mut buff).await?;
    /// //assert_eq!(size, 0);
    ///
    /// let send_result = socket.send(b"hello world").await;
    /// assert!(send_result.is_err());
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.io.shutdown(how)
    }
}

impl TryFrom<std::os::unix::net::UnixDatagram> for UnixDatagram {
    type Error = io::Error;

    /// Consumes stream, returning the Tokio I/O object.
    ///
    /// This is equivalent to
    /// [`UnixDatagram::from_std(stream)`](UnixDatagram::from_std).
    fn try_from(stream: std::os::unix::net::UnixDatagram) -> Result<Self, Self::Error> {
        Self::from_std(stream)
    }
}

impl fmt::Debug for UnixDatagram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.fmt(f)
    }
}

impl AsRawFd for UnixDatagram {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}
