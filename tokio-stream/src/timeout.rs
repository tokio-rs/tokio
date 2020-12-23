use crate::{Fuse, Stream};
use tokio::time::{Instant, Sleep};

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::pin_project;
use std::fmt;
use std::time::Duration;

pin_project! {
    /// Stream returned by the [`timeout`](super::StreamExt::timeout) method.
    #[must_use = "streams do nothing unless polled"]
    #[derive(Debug)]
    pub struct Timeout<S> {
        #[pin]
        stream: Fuse<S>,
        #[pin]
        deadline: Sleep,
        duration: Duration,
        poll_deadline: bool,
    }
}

/// Error returned by `Timeout`.
#[derive(Debug, PartialEq)]
pub struct Elapsed(());

impl<S: Stream> Timeout<S> {
    pub(super) fn new(stream: S, duration: Duration) -> Self {
        let next = Instant::now() + duration;
        let deadline = tokio::time::sleep_until(next);

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

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let me = self.project();

        match me.stream.poll_next(cx) {
            Poll::Ready(v) => {
                if v.is_some() {
                    let next = Instant::now() + *me.duration;
                    me.deadline.reset(next);
                    *me.poll_deadline = true;
                }
                return Poll::Ready(v.map(Ok));
            }
            Poll::Pending => {}
        };

        if *me.poll_deadline {
            ready!(me.deadline.poll(cx));
            *me.poll_deadline = false;
            return Poll::Ready(Some(Err(Elapsed::new())));
        }

        Poll::Pending
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

// ===== impl Elapsed =====

impl Elapsed {
    pub(crate) fn new() -> Self {
        Elapsed(())
    }
}

impl fmt::Display for Elapsed {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        "deadline has elapsed".fmt(fmt)
    }
}

impl std::error::Error for Elapsed {}

impl From<Elapsed> for std::io::Error {
    fn from(_err: Elapsed) -> std::io::Error {
        std::io::ErrorKind::TimedOut.into()
    }
}
