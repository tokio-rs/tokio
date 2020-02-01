use crate::stream::Stream;

use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::pin_project;

pin_project! {
    /// Stream for the [`skip_while`](super::StreamExt::skip_while) method.
    #[must_use = "streams do nothing unless polled"]
    pub struct SkipWhile<St, F> {
        #[pin]
        stream: St,
        predicate: F,
        ready: bool,
    }
}

impl<St, F> fmt::Debug for SkipWhile<St, F>
where
    St: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkipWhile")
            .field("stream", &self.stream)
            .field("ready", &self.ready)
            .finish()
    }
}

impl<St, F> SkipWhile<St, F> {
    pub(super) fn new(stream: St, predicate: F) -> Self {
        Self {
            stream,
            predicate,
            ready: false,
        }
    }
}

impl<St, F> Stream for SkipWhile<St, F>
where
    St: Stream,
    F: FnMut(&St::Item) -> bool,
{
    type Item = St::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.ready {
            self.as_mut().project().stream.poll_next(cx)
        } else {
            loop {
                match ready!(self.as_mut().project().stream.poll_next(cx)) {
                    Some(item) => {
                        if !(*self.as_mut().project().predicate)(&item) {
                            *self.as_mut().project().ready = true;
                            return Poll::Ready(Some(item));
                        }
                    }
                    None => return Poll::Ready(None),
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.stream.size_hint();

        if self.ready {
            return (lower, upper);
        }

        (0, upper)
    }
}
