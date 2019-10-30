#![doc(html_root_url = "https://docs.rs/tokio-tls/0.3.0-alpha.6")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(intra_doc_link_resolution_failure)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

//! Async TLS streams
//!
//! This library is an implementation of TLS streams using the most appropriate
//! system library by default for negotiating the connection. That is, on
//! Windows this library uses SChannel, on OSX it uses SecureTransport, and on
//! other platforms it uses OpenSSL.
//!
//! Each TLS stream implements the `Read` and `Write` traits to interact and
//! interoperate with the rest of the futures I/O ecosystem. Client connections
//! initiated from this crate verify hostnames automatically and by default.
//!
//! This crate primarily exports this ability through two newtypes,
//! `TlsConnector` and `TlsAcceptor`. These newtypes augment the
//! functionality provided by the `native-tls` crate, on which this crate is
//! built. Configuration of TLS parameters is still primarily done through the
//! `native-tls` crate.

use tokio::io::{AsyncRead, AsyncWrite};

use native_tls::{Error, HandshakeError, MidHandshakeTlsStream};
use std::fmt;
use std::future::Future;
use std::io::{self, Read, Write};
use std::marker::Unpin;
use std::pin::Pin;
use std::ptr::null_mut;
use std::task::{Context, Poll};

#[derive(Debug)]
struct AllowStd<S> {
    inner: S,
    context: *mut (),
}

/// A wrapper around an underlying raw stream which implements the TLS or SSL
/// protocol.
///
/// A `TlsStream<S>` represents a handshake that has been completed successfully
/// and both the server and the client are ready for receiving and sending
/// data. Bytes read from a `TlsStream` are decrypted from `S` and bytes written
/// to a `TlsStream` are encrypted when passing through to `S`.
#[derive(Debug)]
pub struct TlsStream<S>(native_tls::TlsStream<AllowStd<S>>);

/// A wrapper around a `native_tls::TlsConnector`, providing an async `connect`
/// method.
#[derive(Clone)]
pub struct TlsConnector(native_tls::TlsConnector);

/// A wrapper around a `native_tls::TlsAcceptor`, providing an async `accept`
/// method.
#[derive(Clone)]
pub struct TlsAcceptor(native_tls::TlsAcceptor);

struct MidHandshake<S>(Option<MidHandshakeTlsStream<AllowStd<S>>>);

enum StartedHandshake<S> {
    Done(TlsStream<S>),
    Mid(MidHandshakeTlsStream<AllowStd<S>>),
}

struct StartedHandshakeFuture<F, S>(Option<StartedHandshakeFutureInner<F, S>>);
struct StartedHandshakeFutureInner<F, S> {
    f: F,
    stream: S,
}

struct Guard<'a, S>(&'a mut TlsStream<S>)
where
    AllowStd<S>: Read + Write;

impl<S> Drop for Guard<'_, S>
where
    AllowStd<S>: Read + Write,
{
    fn drop(&mut self) {
        (self.0).0.get_mut().context = null_mut();
    }
}

// *mut () context is neither Send nor Sync
unsafe impl<S: Send> Send for AllowStd<S> {}
unsafe impl<S: Sync> Sync for AllowStd<S> {}

impl<S> AllowStd<S>
where
    S: Unpin,
{
    fn with_context<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Context<'_>, Pin<&mut S>) -> R,
    {
        unsafe {
            assert!(!self.context.is_null());
            let waker = &mut *(self.context as *mut _);
            f(waker, Pin::new(&mut self.inner))
        }
    }
}

impl<S> Read for AllowStd<S>
where
    S: AsyncRead + Unpin,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.with_context(|ctx, stream| stream.poll_read(ctx, buf)) {
            Poll::Ready(r) => r,
            Poll::Pending => Err(io::Error::from(io::ErrorKind::WouldBlock)),
        }
    }
}

impl<S> Write for AllowStd<S>
where
    S: AsyncWrite + Unpin,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.with_context(|ctx, stream| stream.poll_write(ctx, buf)) {
            Poll::Ready(r) => r,
            Poll::Pending => Err(io::Error::from(io::ErrorKind::WouldBlock)),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.with_context(|ctx, stream| stream.poll_flush(ctx)) {
            Poll::Ready(r) => r,
            Poll::Pending => Err(io::Error::from(io::ErrorKind::WouldBlock)),
        }
    }
}

fn cvt<T>(r: io::Result<T>) -> Poll<io::Result<T>> {
    match r {
        Ok(v) => Poll::Ready(Ok(v)),
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Poll::Pending,
        Err(e) => Poll::Ready(Err(e)),
    }
}

impl<S> TlsStream<S> {
    fn with_context<F, R>(&mut self, ctx: &mut Context<'_>, f: F) -> R
    where
        F: FnOnce(&mut native_tls::TlsStream<AllowStd<S>>) -> R,
        AllowStd<S>: Read + Write,
    {
        self.0.get_mut().context = ctx as *mut _ as *mut ();
        let g = Guard(self);
        f(&mut (g.0).0)
    }

    /// Returns a shared reference to the inner stream.
    pub fn get_ref(&self) -> &S
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        &self.0.get_ref().inner
    }

    /// Returns a mutable reference to the inner stream.
    pub fn get_mut(&mut self) -> &mut S
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        &mut self.0.get_mut().inner
    }
}

impl<S> AsyncRead for TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        // Note that this does not forward to `S` because the buffer is
        // unconditionally filled in by OpenSSL, not the actual object `S`.
        // We're decrypting bytes from `S` into the buffer above!
        false
    }

    fn poll_read(
        mut self: Pin<&mut Self>,
        ctx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.with_context(ctx, |s| cvt(s.read(buf)))
    }
}

impl<S> AsyncWrite for TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        ctx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.with_context(ctx, |s| cvt(s.write(buf)))
    }

    fn poll_flush(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.with_context(ctx, |s| cvt(s.flush()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.with_context(ctx, |s| s.shutdown()) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Poll::Pending,
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

async fn handshake<F, S>(f: F, stream: S) -> Result<TlsStream<S>, Error>
where
    F: FnOnce(
            AllowStd<S>,
        ) -> Result<native_tls::TlsStream<AllowStd<S>>, HandshakeError<AllowStd<S>>>
        + Unpin,
    S: AsyncRead + AsyncWrite + Unpin,
{
    let start = StartedHandshakeFuture(Some(StartedHandshakeFutureInner { f, stream }));

    match start.await {
        Err(e) => Err(e),
        Ok(StartedHandshake::Done(s)) => Ok(s),
        Ok(StartedHandshake::Mid(s)) => MidHandshake(Some(s)).await,
    }
}

impl<F, S> Future for StartedHandshakeFuture<F, S>
where
    F: FnOnce(
            AllowStd<S>,
        ) -> Result<native_tls::TlsStream<AllowStd<S>>, HandshakeError<AllowStd<S>>>
        + Unpin,
    S: Unpin,
    AllowStd<S>: Read + Write,
{
    type Output = Result<StartedHandshake<S>, Error>;

    fn poll(
        mut self: Pin<&mut Self>,
        ctx: &mut Context<'_>,
    ) -> Poll<Result<StartedHandshake<S>, Error>> {
        let inner = self.0.take().expect("future polled after completion");
        let stream = AllowStd {
            inner: inner.stream,
            context: ctx as *mut _ as *mut (),
        };

        match (inner.f)(stream) {
            Ok(mut s) => {
                s.get_mut().context = null_mut();
                Poll::Ready(Ok(StartedHandshake::Done(TlsStream(s))))
            }
            Err(HandshakeError::WouldBlock(mut s)) => {
                s.get_mut().context = null_mut();
                Poll::Ready(Ok(StartedHandshake::Mid(s)))
            }
            Err(HandshakeError::Failure(e)) => Poll::Ready(Err(e)),
        }
    }
}

impl TlsConnector {
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
    pub async fn connect<S>(&self, domain: &str, stream: S) -> Result<TlsStream<S>, Error>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        handshake(move |s| self.0.connect(domain, s), stream).await
    }
}

impl fmt::Debug for TlsConnector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsConnector").finish()
    }
}

impl From<native_tls::TlsConnector> for TlsConnector {
    fn from(inner: native_tls::TlsConnector) -> TlsConnector {
        TlsConnector(inner)
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
    pub async fn accept<S>(&self, stream: S) -> Result<TlsStream<S>, Error>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        handshake(move |s| self.0.accept(s), stream).await
    }
}

impl fmt::Debug for TlsAcceptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsAcceptor").finish()
    }
}

impl From<native_tls::TlsAcceptor> for TlsAcceptor {
    fn from(inner: native_tls::TlsAcceptor) -> TlsAcceptor {
        TlsAcceptor(inner)
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> Future for MidHandshake<S> {
    type Output = Result<TlsStream<S>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut_self = self.get_mut();
        let mut s = mut_self.0.take().expect("future polled after completion");

        s.get_mut().context = cx as *mut _ as *mut ();
        match s.handshake() {
            Ok(stream) => Poll::Ready(Ok(TlsStream(stream))),
            Err(HandshakeError::Failure(e)) => Poll::Ready(Err(e)),
            Err(HandshakeError::WouldBlock(mut s)) => {
                s.get_mut().context = null_mut();
                mut_self.0 = Some(s);
                Poll::Pending
            }
        }
    }
}
