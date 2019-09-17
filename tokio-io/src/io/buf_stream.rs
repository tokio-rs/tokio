use crate::io::{BufReader, BufWriter};
use crate::{AsyncBufRead, AsyncRead, AsyncWrite};
use pin_project::pin_project;
use std::io::{self};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

/// Wraps a type that is [`AsyncWrite`] and [`AsyncRead`], and buffers its input and output.
///
/// It can be excessively inefficient to work directly with something that implements [`AsyncWrite`]
/// and [`AsyncRead`]. For example, every `write`, however small, has to traverse the syscall
/// interface, and similarly, every read has to do the same. The [`BufWriter`] and [`BufReader`]
/// types aid with these problems respectively, but do so in only one direction. `BufStream` wraps
/// one in the other so that both directions are buffered. See their documentation for details.
#[pin_project]
#[derive(Debug)]
pub struct BufStream<RW: AsyncRead + AsyncWrite>(#[pin] BufReader<BufWriter<RW>>);

impl<RW: AsyncRead + AsyncWrite> BufStream<RW> {
    /// Wrap a type in both [`BufWriter`] and [`BufReader`].
    ///
    /// See the documentation for those types and [`BufStream`] for details.
    pub fn new(stream: RW) -> BufStream<RW> {
        BufStream(BufReader::new(BufWriter::new(stream)))
    }
}

impl<RW: AsyncRead + AsyncWrite> AsyncWrite for BufStream<RW> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().0.poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().0.poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().0.poll_shutdown(cx)
    }
}

impl<RW: AsyncRead + AsyncWrite> AsyncRead for BufStream<RW> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.project().0.poll_read(cx, buf)
    }

    // we can't skip unconditionally because of the large buffer case in read.
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.0.prepare_uninitialized_buffer(buf)
    }
}

impl<RW: AsyncBufRead + AsyncRead + AsyncWrite> AsyncBufRead for BufStream<RW> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        self.project_into().0.poll_fill_buf(cx)
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        self.project().0.consume(amt)
    }
}
