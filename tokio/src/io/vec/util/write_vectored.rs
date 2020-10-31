use crate::io::vec::AsyncVectoredWrite;

use pin_project_lite::pin_project;
use std::future::Future;
use std::io::{self, IoSlice};
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// A future to write some data from the slice of buffers
    /// to an `AsyncVectoredWrite`.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct WriteVectored<'a, W: ?Sized> {
        writer: &'a mut W,
        bufs: &'a [IoSlice<'a>],
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

/// Tries to write some bytes from the given `bufs` to the writer in an
/// asynchronous manner, returning a future.
pub(crate) fn write_vectored<'a, W>(
    writer: &'a mut W,
    bufs: &'a [IoSlice<'a>],
) -> WriteVectored<'a, W>
where
    W: AsyncVectoredWrite + Unpin + ?Sized,
{
    WriteVectored {
        writer,
        bufs,
        _pin: PhantomPinned,
    }
}

impl<W> Future for WriteVectored<'_, W>
where
    W: AsyncVectoredWrite + Unpin + ?Sized,
{
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let me = self.project();
        Pin::new(me.writer).poll_write_vectored(cx, me.bufs)
    }
}
