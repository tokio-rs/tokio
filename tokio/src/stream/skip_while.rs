use crate::stream::Stream;

use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::pin_project;

pin_project! {
    /// Stream for the [`take_while`](super::StreamExt::take_while) method.
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
        if !*self.as_mut().project().ready {
            loop {
                match ready!(self.as_mut().project().stream.poll_next(cx)) {
                    Some(item) => {
                        if (*self.as_mut().project().predicate)(&item) {
                            *self.as_mut().project().ready = true;
                            return Poll::Ready(Some(item))
                        }
                    },
                    None => return Poll::Ready(None),
                }
            }
            // self.as_mut().project().stream.poll_next(cx)
        } else {
            self.as_mut().project().stream.poll_next(cx)
        }
        // if !*self.as_mut().project().ready {
        //     self.as_mut().project().stream.poll_next(cx).map(|ready| {
        //         let ready = ready.and_then(|item| {
        //             if !(self.as_mut().project().predicate)(&item) {
        //                 None
        //             } else {
        //                 Some(item)
        //             }
        //         });

        //         if ready.is_none() {
        //             *self.as_mut().project().ready = true;
        //         }

        //         ready
        //     })
        // } else {
        //     Poll::Ready(None)
        // }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.ready {
            return (0, Some(0));
        }

        let (_, upper) = self.stream.size_hint();

        (0, upper)
    }
}
