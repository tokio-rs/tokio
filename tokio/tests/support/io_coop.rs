// Each integration test uses only the helpers it needs.
#![allow(dead_code)]

use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{self, AsyncBufRead, AsyncRead, AsyncWrite, ReadBuf};

/// A test reader that returns one byte at a time and can simulate `Interrupted` errors.
pub struct ByteAtATimeReader<'a> {
    pub data: &'a [u8],
    /// Return N `Interrupted` errors before returning actual data.
    pub interruptions_remaining: usize,
}

impl AsyncRead for ByteAtATimeReader<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.interruptions_remaining > 0 {
            self.interruptions_remaining -= 1;
            return Poll::Ready(Err(io::ErrorKind::Interrupted.into()));
        }
        if !self.data.is_empty() && buf.remaining() > 0 {
            buf.put_slice(&self.data[..1]);
            self.data = &self.data[1..];
        }
        Poll::Ready(Ok(()))
    }
}

impl AsyncBufRead for ByteAtATimeReader<'_> {
    fn poll_fill_buf(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        if self.interruptions_remaining > 0 {
            self.interruptions_remaining -= 1;
            return Poll::Ready(Err(io::ErrorKind::Interrupted.into()));
        }
        let me = self.get_mut();
        Poll::Ready(Ok(&me.data[..me.data.len().min(1)]))
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        self.data = &self.data[amt..];
    }
}

/// A test writer that writes one byte at a time and can simulate `Interrupted` errors.
pub struct ByteAtATimeWriter {
    pub data: Vec<u8>,
    /// Return N `Interrupted` errors before writing actual data.
    pub interruptions_remaining: usize,
}

impl AsyncWrite for ByteAtATimeWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.interruptions_remaining > 0 {
            self.interruptions_remaining -= 1;
            return Poll::Ready(Err(io::ErrorKind::Interrupted.into()));
        }
        let n = buf.len().min(1);
        self.data.extend_from_slice(&buf[..n]);
        Poll::Ready(Ok(n))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
