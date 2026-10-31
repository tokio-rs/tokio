use crate::Stream;

use core::fmt;
use core::future::Future;
use core::pin::Pin;
use core::task::{ready, Context, Poll};
use pin_project_lite::pin_project;

pin_project! {
    /// Stream for the [`filter_map_async`](super::StreamExt::filter_map_async) method.
    #[must_use = "streams do nothing unless polled"]
    pub struct FilterMapAsync<St, Fut, F> {
        #[pin]
        stream: St,
        #[pin]
        future: Option<Fut>,
        f: F,
    }
}

impl<St, Fut, F> fmt::Debug for FilterMapAsync<St, Fut, F>
where
    St: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FilterMapAsync")
            .field("stream", &self.stream)
            .finish()
    }
}

impl<St, Fut, F> FilterMapAsync<St, Fut, F> {
    pub(super) fn new(stream: St, f: F) -> Self {
        FilterMapAsync {
            stream,
            future: None,
            f,
        }
    }
}

impl<T, St, F, Fut> Stream for FilterMapAsync<St, Fut, F>
where
    St: Stream,
    Fut: Future<Output = Option<T>>,
    F: FnMut(St::Item) -> Fut,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let mut me = self.project();

        loop {
            if let Some(future) = me.future.as_mut().as_pin_mut() {
                let item = ready!(future.poll(cx));
                me.future.set(None);
                if let Some(item) = item {
                    return Poll::Ready(Some(item));
                }
            }

            match ready!(me.stream.as_mut().poll_next(cx)) {
                Some(item) => {
                    me.future.set(Some((me.f)(item)));
                }
                None => return Poll::Ready(None),
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let future_len = usize::from(self.future.is_some());
        let upper = self
            .stream
            .size_hint()
            .1
            .and_then(|upper| upper.checked_add(future_len));
        (0, upper)
    }
}
