//! Slow down a stream by enforcing a delay between items.

use crate::stream::Stream;
use crate::time::{Duration, Instant, Sleep};

use std::future::Future;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{self, Poll};

use pin_project_lite::pin_project;

pub(super) fn throttle<T>(duration: Duration, stream: T) -> Throttle<T>
where
    T: Stream,
{
    let delay = if duration == Duration::from_millis(0) {
        None
    } else {
        Some(Sleep::new_timeout(Instant::now() + duration))
    };

    Throttle {
        delay,
        duration,
        has_delayed: true,
        stream,
    }
}

pin_project! {
    /// Stream for the [`throttle`](throttle) function.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct Throttle<T> {
        // `None` when duration is zero.
        delay: Option<Sleep>,
        duration: Duration,

        // Set to true when `delay` has returned ready, but `stream` hasn't.
        has_delayed: bool,

        // The stream to throttle
        #[pin]
        stream: T,
    }
}

// XXX: are these safe if `T: !Unpin`?
impl<T: Unpin> Throttle<T> {
    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    pub fn get_ref(&self) -> &T {
        &self.stream
    }

    /// Acquires a mutable reference to the underlying stream that this combinator
    /// is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the stream
    /// which may otherwise confuse this combinator.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.stream
    }

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so care
    /// should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> T {
        self.stream
    }
}

impl<T: Stream> Stream for Throttle<T> {
    type Item = T::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        if !self.has_delayed && self.delay.is_some() {
            ready!(Pin::new(self.as_mut().project().delay.as_mut().unwrap()).poll(cx));
            *self.as_mut().project().has_delayed = true;
        }

        let value = ready!(self.as_mut().project().stream.poll_next(cx));

        if value.is_some() {
            let dur = self.duration;
            if let Some(ref mut delay) = self.as_mut().project().delay {
                delay.reset(Instant::now() + dur);
            }

            *self.as_mut().project().has_delayed = false;
        }

        Poll::Ready(value)
    }
}
