//! `TcpStream` split support.
//!
//! A `TcpStream` can be split into a `TcpStreamReadHalf` and a
//! `TcpStreamWriteHalf` with the `TcpStream::split` method. `TcpStreamReadHalf`
//! implements `AsyncRead` while `TcpStreamWriteHalf` implements `AsyncWrite`.
//! The two halves can be used concurrently, even from multiple tasks.
//!
//! Compared to the generic split of `AsyncRead + AsyncWrite`, this specialized
//! split gives read and write halves that are faster and smaller, because they
//! do not use locks. They also provide access to the underlying `TcpStream`
//! after split, implementing `AsRef<TcpStream>`. This allows you to call
//! `TcpStream` methods that takes `&self`, e.g., to get local and peer
//! addresses, to get and set socket options, and to shutdown the sockets.

use super::TcpStream;
use bytes::{Buf, BufMut};
use std::error::Error;
use std::fmt;
use std::io;
use std::net::Shutdown;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio_io::{AsyncRead, AsyncWrite};

/// Read half of a `TcpStream`.
#[derive(Debug)]
pub struct TcpStreamReadHalf(Arc<TcpStream>);

/// Write half of a `TcpStream`.
///
/// Note that in the `AsyncWrite` implemenation of `TcpStreamWriteHalf`,
/// `poll_shutdown` actually shuts down the TCP stream in the write direction.
#[derive(Debug)]
pub struct TcpStreamWriteHalf(Arc<TcpStream>);

pub(crate) fn split(stream: TcpStream) -> (TcpStreamReadHalf, TcpStreamWriteHalf) {
    let shared = Arc::new(stream);
    (
        TcpStreamReadHalf(shared.clone()),
        TcpStreamWriteHalf(shared),
    )
}

/// Read half of a `TcpStream`.
#[derive(Debug)]
pub struct TcpStreamReadHalfMut<'a>(&'a TcpStream);

/// Write half of a `TcpStream`.
///
/// Note that in the `AsyncWrite` implemenation of `TcpStreamWriteHalf`,
/// `poll_shutdown` actually shuts down the TCP stream in the write direction.
#[derive(Debug)]
pub struct TcpStreamWriteHalfMut<'a>(&'a TcpStream);

pub(crate) fn split_mut<'a>(
    stream: &'a mut TcpStream,
) -> (TcpStreamReadHalfMut<'a>, TcpStreamWriteHalfMut<'a>) {
    (
        TcpStreamReadHalfMut(&*stream),
        TcpStreamWriteHalfMut(&*stream),
    )
}

/// Error indicating two halves were not from the same stream, and thus could
/// not be `reunite`d.
#[derive(Debug)]
pub struct ReuniteError(pub TcpStreamReadHalf, pub TcpStreamWriteHalf);

impl fmt::Display for ReuniteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tried to reunite halves that are not from the same stream"
        )
    }
}

impl Error for ReuniteError {}

impl TcpStreamReadHalf {
    /// Attempts to put the two "halves" of a `TcpStream` back together and
    /// recover the original stream. Succeeds only if the two "halves"
    /// originated from the same call to `TcpStream::split`.
    pub fn reunite(self, other: TcpStreamWriteHalf) -> Result<TcpStream, ReuniteError> {
        if Arc::ptr_eq(&self.0, &other.0) {
            drop(other);
            // Only two instances of the `Arc` are ever created, one for the
            // reader and one for the writer, and those `Arc`s are never exposed
            // externally. And so when we drop one here, the other one must be
            // the only remaining one.
            Ok(Arc::try_unwrap(self.0).expect("tokio_tcp: try_unwrap failed in reunite"))
        } else {
            Err(ReuniteError(self, other))
        }
    }
}

impl TcpStreamWriteHalf {
    /// Attempts to put the two "halves" of a `TcpStream` back together and
    /// recover the original stream. Succeeds only if the two "halves"
    /// originated from the same call to `TcpStream::split`.
    pub fn reunite(self, other: TcpStreamReadHalf) -> Result<TcpStream, ReuniteError> {
        other.reunite(self)
    }
}

impl AsRef<TcpStream> for TcpStreamReadHalf {
    fn as_ref(&self) -> &TcpStream {
        &self.0
    }
}

impl AsRef<TcpStream> for TcpStreamWriteHalf {
    fn as_ref(&self) -> &TcpStream {
        &self.0
    }
}

impl AsRef<TcpStream> for TcpStreamReadHalfMut<'_> {
    fn as_ref(&self) -> &TcpStream {
        self.0
    }
}

impl AsRef<TcpStream> for TcpStreamWriteHalfMut<'_> {
    fn as_ref(&self) -> &TcpStream {
        self.0
    }
}

impl AsyncRead for TcpStreamReadHalf {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.0.poll_read_priv(cx, buf)
    }

    fn poll_read_buf<B: BufMut>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        self.0.poll_read_buf_priv(cx, buf)
    }
}

impl AsyncWrite for TcpStreamWriteHalf {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.0.poll_write_priv(cx, buf)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        // tcp flush is a no-op
        Poll::Ready(Ok(()))
    }

    // `poll_shutdown` on a write half shutdowns the stream in the "write" direction.
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.0.shutdown(Shutdown::Write).into()
    }

    fn poll_write_buf<B: Buf>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        self.0.poll_write_buf_priv(cx, buf)
    }
}

impl AsyncRead for TcpStreamReadHalfMut<'_> {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.0.poll_read_priv(cx, buf)
    }

    fn poll_read_buf<B: BufMut>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        self.0.poll_read_buf_priv(cx, buf)
    }
}

impl AsyncWrite for TcpStreamWriteHalfMut<'_> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.0.poll_write_priv(cx, buf)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        // tcp flush is a no-op
        Poll::Ready(Ok(()))
    }

    // `poll_shutdown` on a write half shutdowns the stream in the "write" direction.
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.0.shutdown(Shutdown::Write).into()
    }

    fn poll_write_buf<B: Buf>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        self.0.poll_write_buf_priv(cx, buf)
    }
}
