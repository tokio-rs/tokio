use crate::stream::{Fuse, Stream};

use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::pin_project;

pin_project! {
    /// Stream returned by the [`chain`](super::StreamExt::chain) method.
    pub struct Chain<T, U> {
        #[pin]
        a: Fuse<T>,
        #[pin]
        b: U,
    }
}

impl<T, U> Chain<T, U> {
    pub(super) fn new(a: T, b: U) -> Chain<T, U>
    where
        T: Stream,
        U: Stream,
    {
        Chain { a: Fuse::new(a), b }
    }
}

impl<T, U> Stream for Chain<T, U>
where
    T: Stream,
    U: Stream<Item = T::Item>,
{
    type Item = T::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T::Item>> {
        use Poll::Ready;

        let me = self.project();

        if let Some(v) = ready!(me.a.poll_next(cx)) {
            return Ready(Some(v));
        }

        me.b.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (a_lower, a_upper) = self.a.size_hint();
        let (b_lower, b_upper) = self.b.size_hint();

        let upper = match (a_upper, b_upper) {
            (Some(a_upper), Some(b_upper)) => Some(a_upper + b_upper),
            _ => None,
        };

        (a_lower + b_lower, upper)
    }
}
