//! Asynchronous TLS/SSL streams for Tokio using [Rustls](https://github.com/ctz/rustls).
//!
//! Copied almost entirely from https://github.com/quininer/tokio-rustls, thank you!

mod common;

/// The client module serves to separate out the client specific logic for
/// creating and consuming a TLS connection stream.
pub mod client;

/// The server module serves to separate out the server specific logic for
/// accepting and consuming a TLS connection stream.
pub mod server;

use common::Stream;
use rustls_dep::{ClientConfig, ClientSession, ServerConfig, ServerSession, Session};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::{fmt, io, mem};
use tokio::io::{AsyncRead, AsyncWrite};
use webpki::DNSNameRef;

pub(crate) use rustls_dep;
pub(crate) use webpki;

/// TlsState represents the current state of a tls socket
#[derive(Debug)]
pub enum TlsState {
    /// EarlyData represents TLS in 0-RTT state
    #[cfg(feature = "early-data")]
    EarlyData(usize, Vec<u8>),
    /// Stream is the streaming state, i.e. open
    Stream,
    /// ReadShutdown means the stream no longer accepts reads
    ReadShutdown,
    /// WriteShutdown means the stream no longer accepts writes
    WriteShutdown,
    /// FullyShutdown means the stream no longer accepts anything
    FullyShutdown,
}

impl TlsState {
    fn shutdown_read(&mut self) {
        match *self {
            TlsState::WriteShutdown | TlsState::FullyShutdown => *self = TlsState::FullyShutdown,
            _ => *self = TlsState::ReadShutdown,
        }
    }

    fn shutdown_write(&mut self) {
        match *self {
            TlsState::ReadShutdown | TlsState::FullyShutdown => *self = TlsState::FullyShutdown,
            _ => *self = TlsState::WriteShutdown,
        }
    }

    fn writeable(&self) -> bool {
        match *self {
            TlsState::WriteShutdown | TlsState::FullyShutdown => false,
            _ => true,
        }
    }

    fn readable(&self) -> bool {
        match self {
            TlsState::ReadShutdown | TlsState::FullyShutdown => false,
            _ => true,
        }
    }

    #[cfg(feature = "early-data")]
    fn is_early_data(&self) -> bool {
        match self {
            TlsState::EarlyData(..) => true,
            _ => false,
        }
    }

    #[cfg(not(feature = "early-data"))]
    const fn is_early_data(&self) -> bool {
        false
    }
}

/// A wrapper around a `rustls::ClientConfig`, providing an async `connect` method.
#[derive(Clone)]
pub struct TlsConnector {
    inner: Arc<ClientConfig>,
    #[cfg(feature = "early-data")]
    early_data: bool,
}

impl fmt::Debug for TlsConnector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsConnector").finish()
    }
}

/// A wrapper around a `rustls::ServerConfig`, providing an async `accept` method.
#[derive(Clone)]
pub struct TlsAcceptor {
    inner: Arc<ServerConfig>,
}

impl fmt::Debug for TlsAcceptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsAcceptor").finish()
    }
}

impl From<Arc<ClientConfig>> for TlsConnector {
    fn from(inner: Arc<ClientConfig>) -> TlsConnector {
        TlsConnector {
            inner,
            #[cfg(feature = "early-data")]
            early_data: false,
        }
    }
}

impl From<Arc<ServerConfig>> for TlsAcceptor {
    fn from(inner: Arc<ServerConfig>) -> TlsAcceptor {
        TlsAcceptor { inner }
    }
}

impl TlsConnector {
    /// Enable 0-RTT.
    ///
    /// Note that you want to use 0-RTT.
    /// You must set `enable_early_data` to `true` in `ClientConfig`.
    #[cfg(feature = "early-data")]
    pub fn early_data(mut self, flag: bool) -> TlsConnector {
        self.early_data = flag;
        self
    }

    /// Connects the provided stream with this connector, assuming the provided
    /// domain.
    ///
    /// This function will internally call `TlsConnector::connect` to connect
    /// the stream and returns a future representing the resolution of the
    /// connection operation. The returned future will resolve to either
    /// `TlsStream<S>` or `Error` depending if it's successful or not.
    ///
    /// This is typically used for clients who have already established, for
    /// example, a TCP connection to a remote server. That stream is then
    /// provided here to perform the client half of a connection to a
    /// TLS-powered server.
    #[inline]
    pub fn connect<IO>(&self, domain: DNSNameRef<'_>, stream: IO) -> Connect<IO>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
    {
        self.connect_with(domain, stream, |_| ())
    }

    /// Connects and runs the passed function upon generating a new TLS session
    pub fn connect_with<IO, F>(&self, domain: DNSNameRef<'_>, stream: IO, f: F) -> Connect<IO>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
        F: FnOnce(&mut ClientSession),
    {
        let mut session = ClientSession::new(&self.inner, domain);
        f(&mut session);

        Connect(client::MidHandshake::Handshaking(client::TlsStream {
            io: stream,

            #[cfg(not(feature = "early-data"))]
            state: TlsState::Stream,

            #[cfg(feature = "early-data")]
            state: if self.early_data && session.early_data().is_some() {
                TlsState::EarlyData(0, Vec::new())
            } else {
                TlsState::Stream
            },

            session,
        }))
    }
}

impl TlsAcceptor {
    /// Accepts a new client connection with the provided stream.
    ///
    /// This function will internally call `TlsAcceptor::accept` to connect
    /// the stream and returns a future representing the resolution of the
    /// connection operation. The returned future will resolve to either
    /// `TlsStream<S>` or `Error` depending if it's successful or not.
    ///
    /// This is typically used after a new socket has been accepted from a
    /// `TcpListener`. That socket is then passed to this function to perform
    /// the server half of accepting a client connection.
    #[inline]
    pub fn accept<IO>(&self, stream: IO) -> Accept<IO>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
    {
        self.accept_with(stream, |_| ())
    }

    /// Accepts a new client connection and calls the supplied function upon
    /// creation of the session.
    pub fn accept_with<IO, F>(&self, stream: IO, f: F) -> Accept<IO>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
        F: FnOnce(&mut ServerSession),
    {
        let mut session = ServerSession::new(&self.inner);
        f(&mut session);

        Accept(server::MidHandshake::Handshaking(server::TlsStream {
            session,
            io: stream,
            state: TlsState::Stream,
        }))
    }
}

/// Future returned from `TlsConnector::connect` which will resolve
/// once the connection handshake has finished.
pub struct Connect<IO>(client::MidHandshake<IO>);

impl<IO> fmt::Debug for Connect<IO> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connect").finish()
    }
}

/// Future returned from `TlsAcceptor::accept` which will resolve
/// once the accept handshake has finished.
pub struct Accept<IO>(server::MidHandshake<IO>);

impl<IO> fmt::Debug for Accept<IO> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Accept").finish()
    }
}

impl<IO: AsyncRead + AsyncWrite + Unpin> Future for Connect<IO> {
    type Output = io::Result<client::TlsStream<IO>>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

impl<IO: AsyncRead + AsyncWrite + Unpin> Future for Accept<IO> {
    type Output = io::Result<server::TlsStream<IO>>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

/// Unified TLS stream type
///
/// This abstracts over the inner `client::TlsStream` and `server::TlsStream`, so you can use
/// a single type to keep both client- and server-initiated TLS-encrypted connections.
pub enum TlsStream<T> {
    /// Client is the client version of a TlsStream
    Client(client::TlsStream<T>),
    /// Server is the server version of a TlsStream
    Server(server::TlsStream<T>),
}

impl<T> fmt::Debug for TlsStream<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsStream").finish()
    }
}

impl<T> TlsStream<T> {
    /// Returns a shared reference to the inner stream and session.
    pub fn get_ref(&self) -> (&T, &dyn Session) {
        use TlsStream::*;
        match self {
            Client(io) => {
                let (io, session) = io.get_ref();
                (io, &*session)
            }
            Server(io) => {
                let (io, session) = io.get_ref();
                (io, &*session)
            }
        }
    }

    /// Returns a mutable reference to the inner stream and session.
    pub fn get_mut(&mut self) -> (&mut T, &mut dyn Session) {
        use TlsStream::*;
        match self {
            Client(io) => {
                let (io, session) = io.get_mut();
                (io, &mut *session)
            }
            Server(io) => {
                let (io, session) = io.get_mut();
                (io, &mut *session)
            }
        }
    }
}

impl<T> From<client::TlsStream<T>> for TlsStream<T> {
    fn from(s: client::TlsStream<T>) -> Self {
        Self::Client(s)
    }
}

impl<T> From<server::TlsStream<T>> for TlsStream<T> {
    fn from(s: server::TlsStream<T>) -> Self {
        Self::Server(s)
    }
}

impl<T> AsyncRead for TlsStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            TlsStream::Client(x) => Pin::new(x).poll_read(cx, buf),
            TlsStream::Server(x) => Pin::new(x).poll_read(cx, buf),
        }
    }
}

impl<T> AsyncWrite for TlsStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    #[inline]
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            TlsStream::Client(x) => Pin::new(x).poll_write(cx, buf),
            TlsStream::Server(x) => Pin::new(x).poll_write(cx, buf),
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            TlsStream::Client(x) => Pin::new(x).poll_flush(cx),
            TlsStream::Server(x) => Pin::new(x).poll_flush(cx),
        }
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            TlsStream::Client(x) => Pin::new(x).poll_shutdown(cx),
            TlsStream::Server(x) => Pin::new(x).poll_shutdown(cx),
        }
    }
}
