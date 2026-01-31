use crate::io::AsyncWrite;

use pin_project_lite::pin_project;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::{future::Future, io::IoSlice};
use std::{io, mem};

pin_project! {
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct WriteAllVectored<'a, 'b, W: ?Sized> {
        writer: &'a mut W,
        bufs: &'a mut [IoSlice<'b>],
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

pub(crate) fn write_all_vectored<'a, 'b, W>(
    writer: &'a mut W,
    bufs: &'a mut [IoSlice<'b>],
) -> WriteAllVectored<'a, 'b, W>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    WriteAllVectored {
        writer,
        bufs,
        _pin: PhantomPinned,
    }
}

impl<W> Future for WriteAllVectored<'_, '_, W>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    type Output = io::Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let me = self.project();
        while !me.bufs.is_empty() {
            // advance to first non-empty buffer
            let non_empty = match me.bufs.iter().position(|b| !b.is_empty()) {
                Some(pos) => pos,
                None => return Poll::Ready(Ok(())),
            };

            // drop empty buffers at the start
            *me.bufs = &mut mem::take(me.bufs)[non_empty..];

            let n = ready!(Pin::new(&mut *me.writer).poll_write_vectored(cx, me.bufs))?;
            if n == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            IoSlice::advance_slices(me.bufs, n);
        }

        Poll::Ready(Ok(()))
    }
}
