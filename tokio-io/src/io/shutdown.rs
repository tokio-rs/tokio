use crate::AsyncWrite;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future used to shutdown an I/O object.
///
/// Created by the [`AsyncWriteExt::shutdown`] function.
///
/// [`shutdown`]: fn.shutdown.html
#[derive(Debug)]
pub struct Shutdown<'a, A: ?Sized> {
    a: &'a mut A,
}

/// Creates a future which will shutdown an I/O object.
pub(super) fn shutdown<A>(a: &mut A) -> Shutdown<'_, A>
where
    A: AsyncWrite + Unpin + ?Sized,
{
    Shutdown { a }
}

impl<'a, A> Unpin for Shutdown<'a, A> where A: Unpin + ?Sized {}

impl<A> Future for Shutdown<'_, A>
where
    A: AsyncWrite + Unpin + ?Sized,
{
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = &mut *self;
        Pin::new(&mut *me.a).poll_shutdown(cx)
    }
}
