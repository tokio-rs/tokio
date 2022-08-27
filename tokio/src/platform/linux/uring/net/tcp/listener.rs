use super::TcpStream;
use crate::platform::linux::uring::driver::Socket;
use std::{io, net::SocketAddr};

/// A TCP socket server, listening for connections.
///
/// You can accept a new connection by using the [`accept`](`TcpListener::accept`)
/// method.
///
/// # Examples
///
/// ```no_run
/// use tokio::platform::linux::uring::net::TcpListener;
/// use tokio::platform::linux::uring::net::TcpStream;
///
/// #[tokio::main]
/// async fn main() {
///     let listener = TcpListener::bind("127.0.0.1:2345".parse().unwrap()).unwrap();
///
///     let tx_fut = TcpStream::connect("127.0.0.1:2345".parse().unwrap());
///
///     let rx_fut = listener.accept();
///
///     let (tx, (rx, _)) = tokio::try_join!(tx_fut, rx_fut).unwrap();
///
///     tx.write(b"test" as &'static [u8]).await.0.unwrap();
///
///     let (_, buf) = rx.read(vec![0; 4]).await;
///
///     assert_eq!(buf, b"test");
/// }
/// ```
pub struct TcpListener {
    inner: Socket,
}

impl TcpListener {
    /// Creates a new TcpListener, which will be bound to the specified address.
    ///
    /// The returned listener is ready for accepting connections.
    ///
    /// Binding with a port number of 0 will request that the OS assigns a port
    /// to this listener.
    ///
    /// In the future, the port allocated can be queried via a (blocking) `local_addr`
    /// method.
    pub fn bind(addr: SocketAddr) -> io::Result<Self> {
        let socket = Socket::bind(addr, libc::SOCK_STREAM)?;
        socket.listen(1024)?;
        return Ok(TcpListener { inner: socket });
    }

    /// Accepts a new incoming connection from this listener.
    ///
    /// This function will yield once a new TCP connection is established. When
    /// established, the corresponding [`TcpStream`] and the remote peer's
    /// address will be returned.
    ///
    /// [`TcpStream`]: struct@crate::net::TcpStream
    pub async fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let (socket, socket_addr) = self.inner.accept().await?;
        let stream = TcpStream { inner: socket };
        let socket_addr = socket_addr.ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Could not get socket IP address")
        })?;
        Ok((stream, socket_addr))
    }
}

impl std::fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: Provide better debug information
        f.debug_struct("TcpListener").finish()
    }
}
