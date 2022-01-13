//! TCP/UDP/Unix helpers for tokio.

use crate::either::Either;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(unix)]
pub mod unix;

/// A trait for a listener: `TcpListener` and `UnixListener`.
pub trait Listener: Send + Unpin {
    /// The stream's type of this listener.
    type Io: tokio::io::AsyncRead + tokio::io::AsyncWrite;
    /// The socket address type of this listener.
    type Addr;

    /// Polls to accept a new incoming connection to this listener.
    fn poll_accept(&self, cx: &mut Context<'_>) -> Poll<Result<(Self::Io, Self::Addr)>>;

    /// Accepts a new incoming connection from this listener.
    fn accept(&self) -> ListenerAcceptFut<'_, Self>
    where
        Self: Sized + Unpin,
    {
        ListenerAcceptFut::new(self)
    }

    /// Returns the local address that this listener is bound to.
    fn local_addr(&self) -> Result<Self::Addr>;
}

impl Listener for tokio::net::TcpListener {
    type Io = tokio::net::TcpStream;
    type Addr = std::net::SocketAddr;

    fn poll_accept(&self, cx: &mut Context<'_>) -> Poll<Result<(Self::Io, Self::Addr)>> {
        Self::poll_accept(self, cx)
    }

    fn local_addr(&self) -> Result<Self::Addr> {
        self.local_addr().map(Into::into)
    }
}

/// Future for accepting a new connection from a listener.
#[derive(Debug)]
pub struct ListenerAcceptFut<'a, L> {
    listener: &'a L,
}

impl<'a, L> ListenerAcceptFut<'a, L>
where
    L: Listener,
{
    fn new(listener: &'a L) -> Self {
        Self { listener }
    }
}

impl<'a, L> Future for ListenerAcceptFut<'a, L>
where
    L: Listener,
{
    type Output = Result<(L::Io, L::Addr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.listener.poll_accept(cx)
    }
}

impl<L, R> Listener for Either<L, R>
where
    L: Listener,
    R: Listener,
{
    type Io = Either<<L as Listener>::Io, <R as Listener>::Io>;
    type Addr = Either<<L as Listener>::Addr, <R as Listener>::Addr>;

    fn poll_accept(&self, cx: &mut Context<'_>) -> Poll<Result<(Self::Io, Self::Addr)>> {
        match self {
            Either::Left(listener) => listener
                .poll_accept(cx)
                .map(|res| res.map(|(stream, addr)| (Either::Left(stream), Either::Left(addr)))),
            Either::Right(listener) => listener
                .poll_accept(cx)
                .map(|res| res.map(|(stream, addr)| (Either::Right(stream), Either::Right(addr)))),
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
