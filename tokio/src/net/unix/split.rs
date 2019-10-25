//! `UnixStream` split support.
//!
//! A `UnixStream` can be split into a read half and a write half with
//! `UnixStream::split`. The read half implements `AsyncRead` while the write
//! half implements `AsyncWrite`.
//!
//! Compared to the generic split of `AsyncRead + AsyncWrite`, this specialized
//! split has no associated overhead and enforces all invariants at the type
//! level.

use super::UnixStream;

use tokio_io::{AsyncRead, AsyncWrite};

use bytes::{Buf, BufMut};
use std::io;
use std::net::Shutdown;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Read half of a `UnixStream`.
#[derive(Debug)]
pub struct ReadHalf<'a>(&'a UnixStream);

/// Write half of a `UnixStream`.
#[derive(Debug)]
pub struct WriteHalf<'a>(&'a UnixStream);

pub(crate) fn split(stream: &mut UnixStream) -> (ReadHalf<'_>, WriteHalf<'_>) {
    (ReadHalf(stream), WriteHalf(stream))
}

impl AsyncRead for ReadHalf<'_> {
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

impl AsyncWrite for WriteHalf<'_> {
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

impl AsRef<UnixStream> for ReadHalf<'_> {
    fn as_ref(&self) -> &UnixStream {
        self.0
    }
}

impl AsRef<UnixStream> for WriteHalf<'_> {
    fn as_ref(&self) -> &UnixStream {
        self.0
    }
}
