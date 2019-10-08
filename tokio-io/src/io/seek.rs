use crate::AsyncSeek;
use futures_core::future::Future;
use futures_core::task::{Context, Poll};
use std::io::{self, SeekFrom};
use std::pin::Pin;

/// Future for the [`seek`](crate::io::AsyncSeekExt::seek) method.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Seek<'a, S: ?Sized> {
    seek: &'a mut S,
    pos: SeekFrom,
}

pub(super) fn seek<S>(seek: &mut S, pos: SeekFrom) -> Seek<'_, S>
where
    S: AsyncSeek + ?Sized + Unpin,
{
    Seek { seek, pos }
}

impl<S> Future for Seek<'_, S>
where
    S: AsyncSeek + ?Sized + Unpin,
{
    type Output = io::Result<u64>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = &mut *self;
        Pin::new(&mut me.seek).poll_seek(cx, me.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        use std::marker::PhantomPinned;
        crate::is_unpin::<Seek<'_, PhantomPinned>>();
    }
}
