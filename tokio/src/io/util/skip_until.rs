use crate::io::AsyncBufRead;
use crate::util::memchr;

use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::marker::PhantomPinned;
use std::mem;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

pin_project! {
    /// Future for the [`skip_until`](crate::io::AsyncBufReadExt::skip_until) method.
    /// The delimiter is included in the resulting vector.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct SkipUntil<'a, R: ?Sized> {
        reader: &'a mut R,
        delimiter: u8,
        // The number of bytes skipped.
        read: usize,
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

pub(crate) fn skip_until<'a, R>(reader: &'a mut R, delimiter: u8) -> SkipUntil<'a, R>
where
    R: AsyncBufRead + ?Sized + Unpin,
{
    SkipUntil {
        reader,
        delimiter,
        read: 0,
        _pin: PhantomPinned,
    }
}

pub(super) fn skip_until_internal<R: AsyncBufRead + ?Sized>(
    mut reader: Pin<&mut R>,
    cx: &mut Context<'_>,
    delimiter: u8,
    read: &mut usize,
) -> Poll<io::Result<usize>> {
    loop {
        let (done, used) = {
            let available = ready!(reader.as_mut().poll_fill_buf(cx))?;
            if let Some(i) = memchr::memchr(delimiter, available) {
                (true, i + 1)
            } else {
                (false, available.len())
            }
        };
        reader.as_mut().consume(used);
        *read += used;
        if done || used == 0 {
            return Poll::Ready(Ok(mem::replace(read, 0)));
        }
    }
}

impl<R: AsyncBufRead + ?Sized + Unpin> Future for SkipUntil<'_, R> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();
        skip_until_internal(Pin::new(*me.reader), cx, *me.delimiter, me.read)
    }
}
