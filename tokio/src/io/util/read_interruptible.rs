use crate::io::{AsyncBufRead, AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};

use pin_project_lite::pin_project;
use std::future::Future;
use std::io::{self, IoSlice, SeekFrom};
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// `AsyncRead` implementation for the [`read_interruptible`](super::AsyncReadExt::read_interruptible) method.
    #[derive(Debug)]
    #[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
    #[project = ReadInterruptibleProject]
    pub struct ReadInterruptible<R, F> {
        #[pin]
        inner: R,
        #[pin]
        signal: Option<F>,
    }
}

pub(super) fn read_interruptible<R, F>(inner: R, signal: F) -> ReadInterruptible<R, F>
where
    R: AsyncRead,
{
    ReadInterruptible {
        inner,
        signal: Some(signal),
    }
}

impl<R, F> ReadInterruptible<R, F>
where
    R: AsyncRead,
{
    /// Gets a reference to the underlying I/O.
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Gets a mutable reference to the underlying I/O.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Gets a pinned mutable reference to the underlying I/O.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut R> {
        self.project().inner
    }

    /// Consumes the `ReadInterruptible`, returning the wrapped I/O.
    ///
    /// This drops the signaling future regardless of whether it has resolved yet.
    pub fn into_inner(self) -> R {
        self.inner
    }

    /// Indicates whether the reader has already been stopped by the signaling future.
    ///
    /// Note that writing or seeking – if supported by the inner I/O – will still work
    /// even after reading is stopped.
    pub fn is_stopped(&self) -> bool {
        self.signal.is_none()
    }
}

impl<'pin, R, F> ReadInterruptibleProject<'pin, R, F>
where
    F: Future,
{
    /// Polls the `signal` future (if any) and returns whether EOF has been signaled.
    fn poll_eof(&mut self, cx: &mut Context<'_>) -> bool {
        let eof = if let Some(ft) = self.signal.as_mut().as_pin_mut() {
            ft.poll(cx).is_ready()
        } else {
            true
        };

        if eof {
            self.signal.set(None);
        }

        eof
    }
}

impl<R, F> AsyncRead for ReadInterruptible<R, F>
where
    R: AsyncRead,
    F: Future,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut this = self.project();

        if this.poll_eof(cx) {
            Poll::Ready(Ok(()))
        } else {
            this.inner.poll_read(cx, buf)
        }
    }
}

impl<R, F> AsyncBufRead for ReadInterruptible<R, F>
where
    R: AsyncBufRead,
    F: Future,
{
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let mut this = self.project();

        if this.poll_eof(cx) {
            Poll::Ready(Ok(&[]))
        } else {
            this.inner.poll_fill_buf(cx)
        }
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        self.project().inner.consume(amt);
    }
}

impl<R, F> AsyncWrite for ReadInterruptible<R, F>
where
    R: AsyncWrite,
    F: Future,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

impl<R, F> AsyncSeek for ReadInterruptible<R, F>
where
    R: AsyncSeek,
    F: Future,
{
    fn start_seek(self: Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
        self.project().inner.start_seek(position)
    }

    fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        self.project().inner.poll_complete(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        crate::is_unpin::<ReadInterruptible<(), ()>>();
    }
}
