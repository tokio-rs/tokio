//! Unix domain socket helpers.

use std::future::Future;
use std::io::Result;
use std::pin::Pin;

/// Listening address.
#[derive(Debug)]
pub enum ListenAddr {
    /// Socket address.
    SocketAddr(std::net::SocketAddr),
    /// Unix socket.
    UnixSocket(tokio::net::unix::SocketAddr),
}

impl From<std::net::SocketAddr> for ListenAddr {
    fn from(addr: std::net::SocketAddr) -> Self {
        Self::SocketAddr(addr)
    }
}

impl From<tokio::net::unix::SocketAddr> for ListenAddr {
    fn from(addr: tokio::net::unix::SocketAddr) -> Self {
        Self::UnixSocket(addr)
    }
}

/// A trait for a listener: `TcpListener` and `UnixListener`.
pub trait Listener: Send + Unpin {
    /// Accepts a new incoming connection from this listener.
    fn accept<'a>(
        &'a self,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<(Box<dyn AsyncReadWrite + Send + Unpin + 'static>, ListenAddr)>,
                > + Send
                + 'a,
        >,
    >;

    /// Returns the local address that this listener is bound to.
    fn local_addr(&self) -> Result<ListenAddr>;
}

impl Listener for tokio::net::TcpListener {
    fn accept<'a>(
        &'a self,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<(Box<dyn AsyncReadWrite + Send + Unpin + 'static>, ListenAddr)>,
                > + Send
                + 'a,
        >,
    > {
        let accept = self.accept();
        Box::pin(async move {
            let (stream, addr) = accept.await?;
            Ok((Box::new(stream) as Box<_>, addr.into()))
        })
    }

    fn local_addr(&self) -> Result<ListenAddr> {
        self.local_addr().map(Into::into)
    }
}

impl Listener for tokio::net::UnixListener {
    fn accept<'a>(
        &'a self,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<(Box<dyn AsyncReadWrite + Send + Unpin + 'static>, ListenAddr)>,
                > + Send
                + 'a,
        >,
    > {
        let accept = self.accept();
        Box::pin(async {
            let (stream, addr) = accept.await?;
            Ok((Box::new(stream) as Box<_>, addr.into()))
        })
    }

    fn local_addr(&self) -> Result<ListenAddr> {
        self.local_addr().map(Into::into)
    }
}

/// A trait that combines `tokio::io::AsyncRead` and `tokio::io::AsyncWrite`.
pub trait AsyncReadWrite: tokio::io::AsyncRead + tokio::io::AsyncWrite {
    /// Sets the value of the `TCP_NODELAY` option on this socket if this is a TCP socket.
    fn set_nodelay(&self, nodelay: bool) -> Result<()>;
}

impl AsyncReadWrite for tokio::net::TcpStream {
    fn set_nodelay(&self, nodelay: bool) -> Result<()> {
        self.set_nodelay(nodelay)
    }
}

impl AsyncReadWrite for tokio::net::UnixStream {
    fn set_nodelay(&self, _nodelay: bool) -> Result<()> {
        Ok(())
    }
}
