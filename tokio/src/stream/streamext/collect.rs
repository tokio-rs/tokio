use crate::stream::Stream;

use core::future::Future;
use core::mem;
use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::{pin_project};

pin_project! {
/// Future for the [`collect`](super::StreamExt::collect) method.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Collect<St, C> {
    #[pin]
    stream: St,
    collection: C,
}
}

impl<St: Stream, C: Default> Collect<St, C> {
    fn finish(self: Pin<&mut Self>) -> C {
        mem::replace(self.project().collection, Default::default())
    }

    pub(super) fn new(stream: St) -> Collect<St, C> {
        Collect {
            stream,
            collection: Default::default(),
        }
    }
}

impl<St, C> Future for Collect<St, C>
where St: Stream,
      C: Default + Extend<St:: Item>
{
    type Output = C;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<C> {
        loop {
            match ready!(self.as_mut().project().stream.poll_next(cx)) {
                Some(e) => self.as_mut().project().collection.extend(Some(e)),
                None => return Poll::Ready(self.as_mut().finish()),
            }
        }
    }
}