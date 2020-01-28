//! Compatibility between the `tokio::io` and `futures-io` versions of the
//! `AsyncRead` and `AsyncWrite` traits.
use pin_project_lite::pin_project;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// A compatibility layer that allows conversion between the
    /// `tokio::io` and `futures-io` `AsyncRead` and `AsyncWrite` traits.
    #[derive(Copy, Clone, Debug)]
    pub struct Compat<T> {
        #[pin]
        inner: T,
    }
}

/// Extension trait that allows converting a type implementing
/// `futures_io::AsyncRead` to implement `tokio::io::AsyncRead`.
pub trait FuturesAsyncReadCompatExt: futures_io::AsyncRead {
    /// Wraps `self` with a compatibility layer that implements
    /// `tokio_io::AsyncWrite`.
    fn compat(self) -> Compat<Self>
    where
        Self: Sized,
    {
        Compat::new(self)
    }
}

impl<T: futures_io::AsyncRead> FuturesAsyncReadCompatExt for T {}

/// Extension trait that allows converting a type implementing
/// `futures_io::AsyncWrite` to implement `tokio::io::AsyncWrite`.
pub trait FuturesAsyncWriteCompatExt: futures_io::AsyncWrite {
    /// Wraps `self` with a compatibility layer that implements
    /// `tokio::io::AsyncWrite`.
    fn compat_write(self) -> Compat<Self>
    where
        Self: Sized,
    {
        Compat::new(self)
    }
}

impl<T: futures_io::AsyncWrite> FuturesAsyncWriteCompatExt for T {}

/// Extension trait that allows converting a type implementing
/// `tokio::io::AsyncRead` to implement `futures_io::AsyncRead`.
pub trait Tokio02AsyncReadCompatExt: tokio_02::io::AsyncRead {
    /// Wraps `self` with a compatibility layer that implements
    /// `futures_io::AsyncRead`.
    fn compat(self) -> Compat<Self>
    where
        Self: Sized,
    {
        Compat::new(self)
    }
}

impl<T: tokio_02::io::AsyncRead> Tokio02AsyncReadCompatExt for T {}

/// Extension trait that allows converting a type implementing
/// `tokio::io::AsyncWrite` to implement `futures_io::AsyncWrite`.
pub trait Tokio02AsyncWriteCompatExt: tokio_02::io::AsyncWrite {
    /// Wraps `self` with a compatibility layer that implements
    /// `futures_io::AsyncWrite`.
    fn compat_write(self) -> Compat<Self>
    where
        Self: Sized,
    {
        Compat::new(self)
    }
}

impl<T: tokio_02::io::AsyncWrite> Tokio02AsyncWriteCompatExt for T {}

// === impl Compat ===

impl<T> Compat<T> {
    fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T> tokio_02::io::AsyncRead for Compat<T>
where
    T: futures_io::AsyncRead,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        futures_io::AsyncRead::poll_read(self.project().inner, cx, buf)
    }
}

impl<T> futures_io::AsyncRead for Compat<T>
where
    T: tokio_02::io::AsyncRead,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        tokio_02::io::AsyncRead::poll_read(self.project().inner, cx, buf)
    }
}

impl<T> tokio_02::io::AsyncBufRead for Compat<T>
where
    T: futures_io::AsyncBufRead,
{
    fn poll_fill_buf<'a>(
        self: Pin<&'a mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<&'a [u8]>> {
        futures_io::AsyncBufRead::poll_fill_buf(self.project().inner, cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        futures_io::AsyncBufRead::consume(self.project().inner, amt)
    }
}

impl<T> futures_io::AsyncBufRead for Compat<T>
where
    T: tokio_02::io::AsyncBufRead,
{
    fn poll_fill_buf<'a>(
        self: Pin<&'a mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<&'a [u8]>> {
        tokio_02::io::AsyncBufRead::poll_fill_buf(self.project().inner, cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        tokio_02::io::AsyncBufRead::consume(self.project().inner, amt)
    }
}

impl<T> tokio_02::io::AsyncWrite for Compat<T>
where
    T: futures_io::AsyncWrite,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        futures_io::AsyncWrite::poll_write(self.project().inner, cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        futures_io::AsyncWrite::poll_flush(self.project().inner, cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        futures_io::AsyncWrite::poll_close(self.project().inner, cx)
    }
}

impl<T> futures_io::AsyncWrite for Compat<T>
where
    T: tokio_02::io::AsyncWrite,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        tokio_02::io::AsyncWrite::poll_write(self.project().inner, cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        tokio_02::io::AsyncWrite::poll_flush(self.project().inner, cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        tokio_02::io::AsyncWrite::poll_shutdown(self.project().inner, cx)
    }
}
