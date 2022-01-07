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
    /// The stream's type of this listener.
    type Io: tokio::io::AsyncRead + tokio::io::AsyncWrite;

    /// Accepts a new incoming connection from this listener.
    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(Self::Io, ListenAddr)>> + Send + 'a>>;

    /// Returns the local address that this listener is bound to.
    fn local_addr(&self) -> Result<ListenAddr>;
}

impl Listener for tokio::net::TcpListener {
    type Io = tokio::net::TcpStream;

    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(Self::Io, ListenAddr)>> + Send + 'a>> {
        let accept = self.accept();
        Box::pin(async {
            let (stream, addr) = accept.await?;
            Ok((stream, addr.into()))
        })
    }

    fn local_addr(&self) -> Result<ListenAddr> {
        self.local_addr().map(Into::into)
    }
}

impl Listener for tokio::net::UnixListener {
    type Io = tokio::net::UnixStream;

    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(Self::Io, ListenAddr)>> + Send + 'a>> {
        let accept = self.accept();
        Box::pin(async {
            let (stream, addr) = accept.await?;
            Ok((stream, addr.into()))
        })
    }

    fn local_addr(&self) -> Result<ListenAddr> {
        self.local_addr().map(Into::into)
    }
}
