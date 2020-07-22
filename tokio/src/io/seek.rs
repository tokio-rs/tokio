use crate::io::AsyncSeek;
use std::future::Future;
use std::io::{self, SeekFrom};
use std::pin::Pin;
use std::task::{Context, Poll};

/// Future for the [`seek`](crate::io::AsyncSeekExt::seek) method.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Seek<'a, S: ?Sized> {
    seek: &'a mut S,
    pos: Option<SeekFrom>,
}

pub(crate) fn seek<S>(seek: &mut S, pos: SeekFrom) -> Seek<'_, S>
where
    S: AsyncSeek + ?Sized + Unpin,
{
    Seek {
        seek,
        pos: Some(pos),
    }
}

impl<S> Future for Seek<'_, S>
where
    S: AsyncSeek + ?Sized + Unpin,
{
    type Output = io::Result<u64>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = &mut *self;
        match me.pos {
            Some(pos) => match Pin::new(&mut me.seek).start_seek(cx, pos) {
                Poll::Ready(Ok(())) => {
                    me.pos = None;
                    Pin::new(&mut me.seek).poll_complete(cx)
                }
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            },
            None => Pin::new(&mut me.seek).poll_complete(cx),
        }
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
