use crate::AsyncWrite;
use std::future::Future;
use std::io;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future used to fully flush an I/O object.
///
/// Resolves to the underlying I/O object once the flush operation is complete.
///
/// Created by the [`flush`] function.
///
/// [`flush`]: fn.flush.html
#[derive(Debug)]
pub struct Flush<'w, W: Unpin + ?Sized> {
    writer: &'w mut W,
}

// forward the Unpin
impl<W: Unpin + ?Sized> Unpin for Flush<'_, W> {}

/// Creates a future which will entirely flush an I/O object and then yield the
/// object itself.
///
/// This function will consume the object provided if an error happens, and
/// otherwise it will repeatedly call `flush` until it sees `Ok(())`, scheduling
/// a retry if `WouldBlock` is seen along the way.
pub fn flush<W>(writer: &mut W) -> Flush<'_, W>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    Flush { writer }
}

impl<W> Future for Flush<'_, W>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.writer).poll_flush(cx)
    }
}
