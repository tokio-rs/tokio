//! Slow down a stream by enforcing a delay between items.

use crate::stream::Stream;
use crate::time::{Duration, Instant, Sleep};

use std::future::Future;

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

impl<T> Throttle<T> {
    fn get_sleep_mut(self: Pin<&mut Self>) -> Option<Pin<&mut Sleep>> {
        unsafe {
            // SAFETY: We project here to the inner Sleep object. Because we
            // allow access only via a Pin<&mut Sleep>, our returned reference
            // does not allow the pinning guarantees to be violated.

            self.get_unchecked_mut()
                .delay
                .as_mut()
                .map(|delay| Pin::new_unchecked(delay))
        }
    }

    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    pub fn get_ref(&self) -> &T {
        &self.stream
    }

    // The following combinators are safe even if T is !Unpin, because we take
    // an unpinned Self.

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
        if !self.has_delayed {
            if let Some(_sleep) = self.as_mut().get_sleep_mut() {
                ready!(self.as_mut().get_sleep_mut().unwrap().poll(cx));
                *self.as_mut().project().has_delayed = true;
            }
        }

        let value = ready!(self.as_mut().project().stream.poll_next(cx));

        if value.is_some() {
            let dur = self.duration;
            if let Some(ref mut delay) = self.as_mut().get_sleep_mut() {
                delay.as_mut().reset(Instant::now() + dur);
            }

            *self.as_mut().project().has_delayed = false;
        }

        Poll::Ready(value)
    }
}
