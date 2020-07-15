use crate::stream::{Next, Stream};

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

/// Future for the [`try_next`](super::StreamExt::try_next) method.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct TryNext<'a, St: ?Sized> {
    inner: Next<'a, St>,
}

impl<St: ?Sized + Unpin> Unpin for TryNext<'_, St> {}

impl<'a, St: ?Sized> TryNext<'a, St> {
    pub(super) fn new(stream: &'a mut St) -> Self {
        Self {
            inner: Next::new(stream),
        }
    }
}

impl<T, E, St: ?Sized + Stream<Item = Result<T, E>> + Unpin> Future for TryNext<'_, St> {
    type Output = Result<Option<T>, E>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.inner).poll(cx).map(Option::transpose)
    }
}
