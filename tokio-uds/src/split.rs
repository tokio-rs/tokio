//! `UnixStream` split support.
//!
//! A `UnixStream` can be split into a read half and a write half with `UnixStream::split`
//! and `UnixStream::split_mut` methods. The read half implements `AsyncRead` while
//! the write half implements `AsyncWrite`. The two halves can be used concurrently.
//!
//! Compared to the generic split of `AsyncRead + AsyncWrite`, this specialized
//! split gives read and write halves that are faster and smaller, because they
//! do not use locks. They also provide access to the underlying `UnixStream`
//! after split, implementing `AsRef<UnixStream>`. This allows you to call
//! `UnixStream` methods that takes `&self`, e.g., to get local and peer
//! addresses, to get and set socket options, and to shutdown the sockets.

use super::UnixStream;
use bytes::{Buf, BufMut};
use std::io;
use std::net::Shutdown;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio_io::{AsyncRead, AsyncWrite};

/// Read half of a `UnixStream`.
#[derive(Debug)]
pub struct UnixStreamReadHalf(Arc<UnixStream>);

/// Write half of a `UnixStream`.
///
/// Note that in the `AsyncWrite` implementation of `UnixStreamWriteHalf`,
/// `poll_shutdown` actually shuts down the stream in the write direction.
#[derive(Debug)]
pub struct UnixStreamWriteHalf(Arc<UnixStream>);

/// Read half of a `UnixStream`.
#[derive(Debug)]
pub struct UnixStreamReadHalfMut<'a>(&'a UnixStream);

/// Write half of a `UnixStream`.
///
/// Note that in the `AsyncWrite` implementation of `UnixStreamWriteHalfMut`,
/// `poll_shutdown` actually shuts down the stream in the write direction.
#[derive(Debug)]
pub struct UnixStreamWriteHalfMut<'a>(&'a UnixStream);

pub(crate) fn split(stream: UnixStream) -> (UnixStreamReadHalf, UnixStreamWriteHalf) {
    let shared = Arc::new(stream);
    (
        UnixStreamReadHalf(shared.clone()),
        UnixStreamWriteHalf(shared),
    )
}

pub(crate) fn split_mut(
    stream: &mut UnixStream,
) -> (UnixStreamReadHalfMut<'_>, UnixStreamWriteHalfMut<'_>) {
    (
        UnixStreamReadHalfMut(stream),
        UnixStreamWriteHalfMut(stream),
    )
}

impl AsRef<UnixStream> for UnixStreamReadHalf {
    fn as_ref(&self) -> &UnixStream {
        &self.0
    }
}

impl AsRef<UnixStream> for UnixStreamWriteHalf {
    fn as_ref(&self) -> &UnixStream {
        &self.0
    }
}

impl AsRef<UnixStream> for UnixStreamReadHalfMut<'_> {
    fn as_ref(&self) -> &UnixStream {
        self.0
    }
}

impl AsRef<UnixStream> for UnixStreamWriteHalfMut<'_> {
    fn as_ref(&self) -> &UnixStream {
        self.0
    }
}

impl AsyncRead for UnixStreamReadHalf {
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

impl AsyncRead for UnixStreamReadHalfMut<'_> {
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

impl AsyncWrite for UnixStreamWriteHalf {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.0.poll_write_priv(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

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

impl AsyncWrite for UnixStreamWriteHalfMut<'_> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.0.poll_write_priv(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

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
