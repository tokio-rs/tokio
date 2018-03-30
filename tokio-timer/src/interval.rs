use Delay;

use futures::{Future, Stream, Poll};

use std::time::{Instant, Duration};

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
    /// The `duration` argument must be a non-zero duration.
    ///
    /// # Panics
    ///
    /// This function panics if `duration` is zero.
    pub fn new(at: Instant, duration: Duration) -> Interval {
        assert!(duration > Duration::new(0, 0), "`duration` must be non-zero.");

        Interval::new_with_delay(Delay::new(at), duration)
    }

    pub(crate) fn new_with_delay(delay: Delay, duration: Duration) -> Interval {
        Interval {
            delay,
            duration,
        }
    }
}

impl Stream for Interval {
    type Item = Instant;
    type Error = ::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // Wait for the delay to be done
        let _ = try_ready!(self.delay.poll());

        // Get the `now` by looking at the `delay` deadline
        let now = self.delay.deadline();

        // The next interval value is `duration` after the one that just
        // yielded.
        self.delay.reset(now + self.duration);

        // Return the current instant
        Ok(Some(now).into())
    }
}
