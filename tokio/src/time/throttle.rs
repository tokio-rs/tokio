//! Slow down a stream by enforcing a delay between items.

use crate::time::{Delay, Duration, Instant};

use std::future::Future;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{self, Poll};

/// Slow down a stream by enforcing a delay between items.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Throttle<T> {
    /// `None` when duration is zero.
    delay: Option<Delay>,

    /// Set to true when `delay` has returned ready, but `stream` hasn't.
    has_delayed: bool,

    /// The stream to throttle
    stream: T,
}

impl<T> Throttle<T> {
    /// Slow down a stream by enforcing a delay between items.
    pub fn new(stream: T, duration: Duration) -> Self {
        let delay = if duration == Duration::from_millis(0) {
            None
        } else {
            Some(Delay::new_timeout(Instant::now() + duration, duration))
        };

        Self {
            delay,
            has_delayed: true,
            stream,
        }
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
        unsafe {
            if !self.has_delayed && self.delay.is_some() {
                ready!(self
                    .as_mut()
                    .map_unchecked_mut(|me| me.delay.as_mut().unwrap())
                    .poll(cx));
                self.as_mut().get_unchecked_mut().has_delayed = true;
            }

            let value = ready!(self
                .as_mut()
                .map_unchecked_mut(|me| &mut me.stream)
                .poll_next(cx));

            if value.is_some() {
                if let Some(ref mut delay) = self.as_mut().get_unchecked_mut().delay {
                    delay.reset_timeout();
                }

                self.as_mut().get_unchecked_mut().has_delayed = false;
            }

            Poll::Ready(value)
        }
    }
}
