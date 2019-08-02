use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio_io::AsyncWrite;

/// A future used to fully flush an I/O object.
///
/// Created by the [`AsyncWriteExt::flush`] function.
///
/// [`flush`]: fn.flush.html
#[derive(Debug)]
pub struct Flush<'a, A: ?Sized> {
    a: &'a mut A,
}

/// Creates a future which will entirely flush an I/O object.
pub(super) fn flush<A>(a: &mut A) -> Flush<'_, A>
where
    A: AsyncWrite + Unpin + ?Sized,
{
    Flush { a }
}

impl<'a, A> Unpin for Flush<'a, A> where A: Unpin + ?Sized {}

impl<A> Future for Flush<'_, A>
where
    A: AsyncWrite + Unpin + ?Sized,
{
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = &mut *self;
        Pin::new(&mut *me.a).poll_flush(cx)
    }
}
