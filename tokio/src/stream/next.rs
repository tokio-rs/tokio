use crate::stream::Stream;

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

/// Future for the [`next`](super::StreamExt::next) method.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Next<'a, St: ?Sized> {
    stream: &'a mut St,
}

impl<St: ?Sized + Unpin> Unpin for Next<'_, St> {}

impl<'a, St: ?Sized> Next<'a, St> {
    pub(super) fn new(stream: &'a mut St) -> Self {
        Next { stream }
    }
}

impl<St: ?Sized + Stream + Unpin> Future for Next<'_, St> {
    type Output = Option<St::Item>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.stream).poll_next(cx)
    }
}
