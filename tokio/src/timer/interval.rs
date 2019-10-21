use crate::timer::{clock, Delay};

use futures_core::ready;
use futures_util::future::poll_fn;
use std::future::Future;
use std::pin::Pin;
use std::task::{self, Poll};
use std::time::{Duration, Instant};

/// A stream representing notifications at fixed interval
#[derive(Debug)]
pub struct Interval {
    /// Future that completes the next time the `Interval` yields a value.
    delay: Delay,

    /// The duration between values yielded by `Interval`.
    duration: Duration,
}

impl Interval {
    /// Create a new `Interval` that starts at `at` and yields every `duration`
    /// interval after that.
    ///
    /// Note that when it starts, it produces item too.
    ///
    /// The `duration` argument must be a non-zero duration.
    ///
    /// # Panics
    ///
    /// This function panics if `duration` is zero.
    pub fn new(at: Instant, duration: Duration) -> Interval {
        assert!(
            duration > Duration::new(0, 0),
            "`duration` must be non-zero."
        );

        Interval::new_with_delay(Delay::new(at), duration)
    }

    /// Creates new `Interval` that yields with interval of `duration`.
    ///
    /// The function is shortcut for `Interval::new(tokio::timer::clock::now() + duration, duration)`.
    ///
    /// The `duration` argument must be a non-zero duration.
    ///
    /// # Panics
    ///
    /// This function panics if `duration` is zero.
    pub fn new_interval(duration: Duration) -> Interval {
        Interval::new(clock::now() + duration, duration)
    }

    pub(crate) fn new_with_delay(delay: Delay, duration: Duration) -> Interval {
        Interval { delay, duration }
    }

    #[doc(hidden)] // TODO: remove
    pub fn poll_next(&mut self, cx: &mut task::Context<'_>) -> Poll<Option<Instant>> {
        // Wait for the delay to be done
        ready!(Pin::new(&mut self.delay).poll(cx));

        // Get the `now` by looking at the `delay` deadline
        let now = self.delay.deadline();

        // The next interval value is `duration` after the one that just
        // yielded.
        let next = now + self.duration;
        self.delay.reset(next);

        // Return the current instant
        Poll::Ready(Some(now))
    }

    /// Completes when the next instant in the interval has been reached.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::timer::Interval;
    ///
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut interval = Interval::new_interval(Duration::from_millis(10));
    ///
    ///     interval.next().await;
    ///     interval.next().await;
    ///     interval.next().await;
    ///
    ///     // approximately 30ms have elapsed.
    /// }
    /// ```
    #[allow(clippy::should_implement_trait)] // TODO: rename (tokio-rs/tokio#1261)
    pub async fn next(&mut self) -> Option<Instant> {
        poll_fn(|cx| self.poll_next(cx)).await
    }
}

impl futures_core::FusedStream for Interval {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl futures_core::Stream for Interval {
    type Item = Instant;

    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        Interval::poll_next(self.get_mut(), cx)
    }
}
