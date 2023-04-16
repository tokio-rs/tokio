use crate::stream_ext::Fuse;
use crate::{Elapsed, Stream};
use tokio::time::{Instant, Sleep};

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::pin_project;
use std::time::Duration;

pin_project! {
    /// Stream returned by the [`timeout_repeating`](super::StreamExt::timeout_repeating) method.
    #[must_use = "streams do nothing unless polled"]
    #[derive(Debug)]
    pub struct TimeoutRepeating<S> {
        #[pin]
        stream: Fuse<S>,
        #[pin]
        deadline: Sleep,
        duration: Duration,
    }
}

impl<S: Stream> TimeoutRepeating<S> {
    pub(super) fn new(stream: S, duration: Duration) -> Self {
        let next = Instant::now() + duration;
        let deadline = tokio::time::sleep_until(next);

        TimeoutRepeating {
            stream: Fuse::new(stream),
            deadline,
            duration,
        }
    }
}

impl<S: Stream> Stream for TimeoutRepeating<S> {
    type Item = Result<S::Item, Elapsed>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut me = self.project();

        match me.stream.poll_next(cx) {
            Poll::Ready(v) => {
                if v.is_some() {
                    let next = Instant::now() + *me.duration;
                    me.deadline.reset(next);
                }
                return Poll::Ready(v.map(Ok));
            }
            Poll::Pending => {}
        };

        ready!(me.deadline.as_mut().poll(cx));
        let next = Instant::now() + *me.duration;
        me.deadline.reset(next);
        Poll::Ready(Some(Err(Elapsed::new())))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, _) = self.stream.size_hint();

        // The timeout stream may insert an error an infinite number of times.
        (lower, None)
    }
}
