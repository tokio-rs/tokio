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
pub struct BufStream<RW>(#[pin] BufReader<BufWriter<RW>>);

impl<RW: AsyncRead + AsyncWrite> BufStream<RW> {
    /// Wrap a type in both [`BufWriter`] and [`BufReader`].
    ///
    /// See the documentation for those types and [`BufStream`] for details.
    pub fn new(stream: RW) -> BufStream<RW> {
        BufStream(BufReader::new(BufWriter::new(stream)))
    }

    /// Gets a reference to the underlying I/O object.
    ///
    /// It is inadvisable to directly read from the underlying I/O object.
    pub fn get_ref(&self) -> &RW {
        self.0.get_ref().get_ref()
    }

    /// Gets a mutable reference to the underlying I/O object.
    ///
    /// It is inadvisable to directly read from the underlying I/O object.
    pub fn get_mut(&mut self) -> &mut RW {
        self.0.get_mut().get_mut()
    }

    /// Gets a pinned mutable reference to the underlying I/O object.
    ///
    /// It is inadvisable to directly read from the underlying I/O object.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut RW> {
        self.project().0.get_pin_mut().get_pin_mut()
    }

    /// Consumes this `BufStream`, returning the underlying I/O object.
    ///
    /// Note that any leftover data in the internal buffer is lost.
    pub fn into_inner(self) -> RW {
        self.0.into_inner().into_inner()
    }
}

impl<RW> From<BufReader<BufWriter<RW>>> for BufStream<RW> {
    fn from(b: BufReader<BufWriter<RW>>) -> Self {
        BufStream(b)
    }
}

impl<RW> From<BufWriter<BufReader<RW>>> for BufStream<RW> {
    fn from(b: BufWriter<BufReader<RW>>) -> Self {
        // we need to "invert" the reader and writer
        let BufWriter {
            inner:
                BufReader {
                    inner,
                    buf: rbuf,
                    pos,
                    cap,
                },
            buf: wbuf,
            written,
        } = b;

        BufStream(BufReader {
            inner: BufWriter {
                inner,
                buf: wbuf,
                written,
            },
            buf: rbuf,
            pos,
            cap,
        })
    }
}

impl<RW: AsyncRead + AsyncWrite> AsyncWrite for BufStream<RW> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().0.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().0.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().0.poll_shutdown(cx)
    }
}

impl<RW: AsyncRead + AsyncWrite> AsyncRead for BufStream<RW> {
    fn poll_read(
        self: Pin<&mut Self>,
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
        self.project().0.poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        self.project().0.consume(amt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        crate::is_unpin::<BufStream<()>>();
    }
}
