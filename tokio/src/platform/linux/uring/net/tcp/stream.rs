use std::{
    io,
    net::SocketAddr,
    os::unix::prelude::{AsRawFd, RawFd},
};

use crate::platform::linux::uring::{
    buf::{IoBuf, IoBufMut},
    driver::Socket,
};

/// A TCP stream between a local and a remote socket.
///
/// A TCP stream can either be created by connecting to an endpoint, via the
/// [`connect`] method, or by [`accepting`] a connection from a [`listener`].
///
/// # Examples
///
/// ```no_run
/// use tokio::platform::linux::uring::net::TcpStream;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///         // Connect to a peer
///         let stream = TcpStream::connect("127.0.0.1:8080".parse().unwrap()).await?;
///
///         // Write some data.
///         let (result, _) = stream.write(b"hello world!".as_slice()).await;
///         result.unwrap();
///
///         Ok(())
/// }
/// ```
///
/// [`connect`]: TcpStream::connect
/// [`accepting`]: crate::net::TcpListener::accept
/// [`listener`]: crate::net::TcpListener
pub struct TcpStream {
    pub(super) inner: Socket,
}

impl TcpStream {
    /// Opens a TCP connection to a remote host at the given `SocketAddr`
    pub async fn connect(addr: SocketAddr) -> io::Result<TcpStream> {
        let socket = Socket::new(addr, libc::SOCK_STREAM)?;
        socket.connect(socket2::SockAddr::from(addr)).await?;
        let tcp_stream = TcpStream { inner: socket };
        return Ok(tcp_stream);
    }

    /// Read some data from the stream into the buffer, returning the original buffer and
    /// quantity of data read.
    pub async fn read<T: IoBufMut>(&self, buf: T) -> crate::platform::linux::uring::BufResult<usize, T> {
        self.inner.read(buf).await
    }

    /// Write some data to the stream from the buffer, returning the original buffer and
    /// quantity of data written.
    pub async fn write<T: IoBuf>(&self, buf: T) -> crate::platform::linux::uring::BufResult<usize, T> {
        self.inner.write(buf).await
    }
}

impl AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
