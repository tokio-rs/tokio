//! Unix domain socket helpers.

use crate::either::Either;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;

/// A trait for a listener: `TcpListener` and `UnixListener`.
pub trait Listener: Send + Unpin {
    /// The stream's type of this listener.
    type Io: tokio::io::AsyncRead + tokio::io::AsyncWrite;
    /// The socket address type of this listener.
    type Addr;

    /// Accepts a new incoming connection from this listener.
    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(Self::Io, Self::Addr)>> + Send + 'a>>;

    /// Returns the local address that this listener is bound to.
    fn local_addr(&self) -> Result<Self::Addr>;
}

impl Listener for tokio::net::TcpListener {
    type Io = tokio::net::TcpStream;
    type Addr = std::net::SocketAddr;

    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(Self::Io, Self::Addr)>> + Send + 'a>> {
        let accept = self.accept();
        Box::pin(async {
            let (stream, addr) = accept.await?;
            Ok((stream, addr.into()))
        })
    }

    fn local_addr(&self) -> Result<Self::Addr> {
        self.local_addr().map(Into::into)
    }
}

impl Listener for tokio::net::UnixListener {
    type Io = tokio::net::UnixStream;
    type Addr = tokio::net::unix::SocketAddr;

    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(Self::Io, Self::Addr)>> + Send + 'a>> {
        let accept = self.accept();
        Box::pin(async {
            let (stream, addr) = accept.await?;
            Ok((stream, addr.into()))
        })
    }

    fn local_addr(&self) -> Result<Self::Addr> {
        self.local_addr().map(Into::into)
    }
}

impl<L, R> Listener for Either<L, R>
where
    L: Listener,
    R: Listener,
{
    type Io = Either<<L as Listener>::Io, <R as Listener>::Io>;
    type Addr = Either<<L as Listener>::Addr, <R as Listener>::Addr>;

    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(Self::Io, Self::Addr)>> + Send + 'a>> {
        match self {
            Either::Left(listener) => {
                let fut = listener.accept();
                Box::pin(async move {
                    let (stream, addr) = fut.await?;
                    Ok((Either::Left(stream), Either::Left(addr)))
                })
            }
            Either::Right(listener) => {
                let fut = listener.accept();
                Box::pin(async move {
                    let (stream, addr) = fut.await?;
                    Ok((Either::Right(stream), Either::Right(addr)))
                })
            }
        }
    }

    fn local_addr(&self) -> Result<Self::Addr> {
        match self {
            Either::Left(listener) => {
                let addr = listener.local_addr()?;
                Ok(Either::Left(addr))
            }
            Either::Right(listener) => {
                let addr = listener.local_addr()?;
                Ok(Either::Right(addr))
            }
        }
    }
}
