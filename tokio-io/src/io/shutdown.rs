use crate::AsyncWrite;
use std::future::Future;
use std::io;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future used to fully shutdown an I/O object.
///
/// Resolves to the underlying I/O object once the shutdown operation is
/// complete.
///
/// Created by the [`shutdown`] function.
///
/// [`shutdown`]: fn.shutdown.html
#[derive(Debug)]
pub struct Shutdown<'w, W: ?Sized> {
    writer: &'w mut W,
}

/// Creates a future which will entirely shutdown an I/O object and then yield
/// the object itself.
///
/// This function will consume the object provided if an error happens, and
/// otherwise it will repeatedly call `shutdown` until it sees `Ok(())`,
/// scheduling a retry if `WouldBlock` is seen along the way.
pub fn shutdown<W>(writer: &mut W) -> Shutdown<'_, W>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    Shutdown { writer }
}

// forward Unpin
impl<'a, W: Unpin + ?Sized> Unpin for Shutdown<'_, W> {}

impl<W> Future for Shutdown<'_, W>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.writer).poll_shutdown(cx)
    }
}
