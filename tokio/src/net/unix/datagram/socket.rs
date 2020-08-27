use crate::future::poll_fn;
use crate::io::PollEvented;
use crate::net::unix::datagram::split::{split, RecvHalf, SendHalf};
use crate::net::unix::datagram::split_owned::{split_owned, OwnedRecvHalf, OwnedSendHalf};

use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{self, SocketAddr};
use std::path::Path;
use std::task::{Context, Poll};

cfg_uds! {
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
    /// let mut tx = UnixDatagram::bind(&tx_path)?;
    /// let rx_path = tmp.path().join("rx");
    /// let mut rx = UnixDatagram::bind(&rx_path)?;
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
    /// let (mut sock1, mut sock2) = UnixDatagram::pair()?;
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
        io: PollEvented<mio_uds::UnixDatagram>,
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
        let socket = mio_uds::UnixDatagram::bind(path)?;
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
    /// let (mut sock1, mut sock2) = UnixDatagram::pair()?;
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
        let (a, b) = mio_uds::UnixDatagram::pair()?;
        let a = UnixDatagram::new(a)?;
        let b = UnixDatagram::new(b)?;

        Ok((a, b))
    }

    /// Consumes a `UnixDatagram` in the standard library and returns a
    /// nonblocking `UnixDatagram` from this crate.
    ///
    /// The returned datagram will be associated with the given event loop
    /// specified by `handle` and is ready to perform I/O.
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
    /// let tokio_socket = UnixDatagram::from_std(std_socket)?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_std(datagram: net::UnixDatagram) -> io::Result<UnixDatagram> {
        let socket = mio_uds::UnixDatagram::from_datagram(datagram)?;
        let io = PollEvented::new(socket)?;
        Ok(UnixDatagram { io })
    }

    fn new(socket: mio_uds::UnixDatagram) -> io::Result<UnixDatagram> {
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
    /// let mut tx = UnixDatagram::unbound()?;
    ///
    /// // Create another, bound socket
    /// let tmp = tempdir()?;
    /// let rx_path = tmp.path().join("rx");
    /// let mut rx = UnixDatagram::bind(&rx_path)?;
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
        let socket = mio_uds::UnixDatagram::unbound()?;
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
    /// let mut tx = UnixDatagram::unbound()?;
    ///
    /// // Create another, bound socket
    /// let tmp = tempdir()?;
    /// let rx_path = tmp.path().join("rx");
    /// let mut rx = UnixDatagram::bind(&rx_path)?;
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
        self.io.get_ref().connect(path)
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
    /// let (mut sock1, mut sock2) = UnixDatagram::pair()?;
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
    pub async fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        poll_fn(|cx| self.poll_send_priv(cx, buf)).await
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
    /// let (mut first, mut second) = UnixDatagram::pair()?;
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
    pub fn try_send(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.io.get_ref().send(buf)
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
    /// let mut server = UnixDatagram::bind(&server_path)?;
    ///
    /// let client_path = tmp.path().join("client");
    /// let mut client = UnixDatagram::bind(&client_path)?;
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
    pub fn try_send_to<P>(&mut self, buf: &[u8], target: P) -> io::Result<usize>
    where
        P: AsRef<Path>,
    {
        self.io.get_ref().send_to(buf, target)
    }

    // Poll IO functions that takes `&self` are provided for the split API.
    //
    // They are not public because (taken from the doc of `PollEvented`):
    //
    // While `PollEvented` is `Sync` (if the underlying I/O type is `Sync`), the
    // caller must ensure that there are at most two tasks that use a
    // `PollEvented` instance concurrently. One for reading and one for writing.
    // While violating this requirement is "safe" from a Rust memory model point
    // of view, it will result in unexpected behavior in the form of lost
    // notifications and tasks hanging.
    pub(crate) fn poll_send_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_write_ready(cx))?;

        match self.io.get_ref().send(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
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
    /// let (mut sock1, mut sock2) = UnixDatagram::pair()?;
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
    pub async fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        poll_fn(|cx| self.poll_recv_priv(cx, buf)).await
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
    /// let (mut first, mut second) = UnixDatagram::pair()?;
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
    pub fn try_recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.get_ref().recv(buf)
    }

    pub(crate) fn poll_recv_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;

        match self.io.get_ref().recv(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
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
    /// let mut tx = UnixDatagram::bind(&tx_path)?;
    /// let rx_path = tmp.path().join("rx");
    /// let mut rx = UnixDatagram::bind(&rx_path)?;
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
    pub async fn send_to<P>(&mut self, buf: &[u8], target: P) -> io::Result<usize>
    where
        P: AsRef<Path> + Unpin,
    {
        poll_fn(|cx| self.poll_send_to_priv(cx, buf, target.as_ref())).await
    }

    pub(crate) fn poll_send_to_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
        target: &Path,
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_write_ready(cx))?;

        match self.io.get_ref().send_to(buf, target) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
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
    /// let mut tx = UnixDatagram::bind(&tx_path)?;
    /// let rx_path = tmp.path().join("rx");
    /// let mut rx = UnixDatagram::bind(&rx_path)?;
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
    pub async fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        poll_fn(|cx| self.poll_recv_from_priv(cx, buf)).await
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
    /// let mut server = UnixDatagram::bind(&server_path)?;
    ///
    /// let client_path = tmp.path().join("client");
    /// let mut client = UnixDatagram::bind(&client_path)?;
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
    pub fn try_recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.io.get_ref().recv_from(buf)
    }

    pub(crate) fn poll_recv_from_priv(
        &self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, SocketAddr), io::Error>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;

        match self.io.get_ref().recv_from(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
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
        self.io.get_ref().local_addr()
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
        self.io.get_ref().peer_addr()
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
        self.io.get_ref().take_error()
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
    /// let (mut socket, other) = UnixDatagram::pair()?;
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
        self.io.get_ref().shutdown(how)
    }

    // These lifetime markers also appear in the generated documentation, and make
    // it more clear that this is a *borrowed* split.
    #[allow(clippy::needless_lifetimes)]
    /// Split a `UnixDatagram` into a receive half and a send half, which can be used
    /// to receive and send the datagram concurrently.
    ///
    /// This method is more efficient than [`into_split`], but the halves cannot
    /// be moved into independently spawned tasks.
    ///
    /// [`into_split`]: fn@crate::net::UnixDatagram::into_split
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    ///
    /// // Create the pair of sockets
    /// let (mut sock1, mut sock2) = UnixDatagram::pair()?;
    ///
    /// // Split sock1
    /// let (sock1_rx, mut sock1_tx) = sock1.split();
    ///
    /// // Since the sockets are paired, the paired send/recv
    /// // functions can be used
    /// let bytes = b"hello world";
    /// sock1_tx.send(bytes).await?;
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
    pub fn split<'a>(&'a mut self) -> (RecvHalf<'a>, SendHalf<'a>) {
        split(self)
    }

    /// Split a `UnixDatagram` into a receive half and a send half, which can be used
    /// to receive and send the datagram concurrently.
    ///
    /// Unlike [`split`], the owned halves can be moved to separate tasks,
    /// however this comes at the cost of a heap allocation.
    ///
    /// **Note:** Dropping the write half will shut down the write half of the
    /// datagram. This is equivalent to calling [`shutdown(Write)`].
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UnixDatagram;
    ///
    /// // Create the pair of sockets
    /// let (sock1, mut sock2) = UnixDatagram::pair()?;
    ///
    /// // Split sock1
    /// let (sock1_rx, mut sock1_tx) = sock1.into_split();
    ///
    /// // Since the sockets are paired, the paired send/recv
    /// // functions can be used
    /// let bytes = b"hello world";
    /// sock1_tx.send(bytes).await?;
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
    ///
    /// [`split`]: fn@crate::net::UnixDatagram::split
    /// [`shutdown(Write)`]:fn@crate::net::UnixDatagram::shutdown
    pub fn into_split(self) -> (OwnedRecvHalf, OwnedSendHalf) {
        split_owned(self)
    }
}

impl TryFrom<UnixDatagram> for mio_uds::UnixDatagram {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`PollEvented::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    ///
    /// [`PollEvented::into_inner`]: crate::io::PollEvented::into_inner
    fn try_from(value: UnixDatagram) -> Result<Self, Self::Error> {
        value.io.into_inner()
    }
}

impl TryFrom<net::UnixDatagram> for UnixDatagram {
    type Error = io::Error;

    /// Consumes stream, returning the Tokio I/O object.
    ///
    /// This is equivalent to
    /// [`UnixDatagram::from_std(stream)`](UnixDatagram::from_std).
    fn try_from(stream: net::UnixDatagram) -> Result<Self, Self::Error> {
        Self::from_std(stream)
    }
}

impl fmt::Debug for UnixDatagram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl AsRawFd for UnixDatagram {
    fn as_raw_fd(&self) -> RawFd {
        self.io.get_ref().as_raw_fd()
    }
}
