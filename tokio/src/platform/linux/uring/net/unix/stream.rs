use crate::platform::linux::uring::{
    buf::{IoBuf, IoBufMut},
    driver::Socket,
};
use socket2::SockAddr;
use std::{io, path::Path};

/// A Unix stream between two local sockets on a Unix OS.
///
/// A Unix stream can either be created by connecting to an endpoint, via the
/// [`connect`] method, or by [`accepting`] a connection from a [`listener`].
///
/// # Examples
///
/// ```no_run
/// use tokio::platform::linux::uring::net::UnixStream;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///         // Connect to a peer
///         let stream = UnixStream::connect("/tmp/tokio-uring-unix-test.sock").await?;
///
///         // Write some data.
///         let (result, _) = stream.write(b"hello world!".as_slice()).await;
///         result.unwrap();
///
///         Ok(())
/// }
/// ```
///
/// [`connect`]: UnixStream::connect
/// [`accepting`]: crate::net::UnixListener::accept
/// [`listener`]: crate::net::UnixListener
pub struct UnixStream {
    pub(super) inner: Socket,
}

impl UnixStream {
    /// Opens a Unix connection to the specified file path. There must be a
    /// `UnixListener` or equivalent listening on the corresponding Unix domain socket
    /// to successfully connect and return a `UnixStream`.
    pub async fn connect<P: AsRef<Path>>(path: P) -> io::Result<UnixStream> {
        let socket = Socket::new_unix(libc::SOCK_STREAM)?;
        socket.connect(SockAddr::unix(path)?).await?;
        let unix_stream = UnixStream { inner: socket };
        Ok(unix_stream)
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
