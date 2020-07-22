use crate::stream::{Fuse, Stream};
use crate::time::{Delay, Elapsed, Instant};

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::pin_project;
use std::time::Duration;

pin_project! {
    /// Stream returned by the [`timeout`](super::StreamExt::timeout) method.
    #[must_use = "streams do nothing unless polled"]
    #[derive(Debug)]
    pub struct Timeout<S> {
        #[pin]
        stream: Fuse<S>,
        deadline: Delay,
        duration: Duration,
        poll_deadline: bool,
    }
}

impl<S: Stream> Timeout<S> {
    pub(super) fn new(stream: S, duration: Duration) -> Self {
        let next = Instant::now() + duration;
        let deadline = Delay::new_timeout(next, duration);

        Timeout {
            stream: Fuse::new(stream),
            deadline,
            duration,
            poll_deadline: true,
        }
    }
}

impl<S: Stream> Stream for Timeout<S> {
    type Item = Result<S::Item, Elapsed>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.as_mut().project().stream.poll_next(cx) {
            Poll::Ready(v) => {
                if v.is_some() {
                    let next = Instant::now() + self.duration;
                    self.as_mut().project().deadline.reset(next);
                    *self.as_mut().project().poll_deadline = true;
                }
                return Poll::Ready(v.map(Ok));
            }
            Poll::Pending => {}
        };

        if self.poll_deadline {
            ready!(Pin::new(self.as_mut().project().deadline).poll(cx));
            *self.as_mut().project().poll_deadline = false;
            return Poll::Ready(Some(Err(Elapsed::new())));
        }

        Poll::Pending
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}
