use Sleep;

use futures::{Future, Stream, Poll};

use std::time::{Instant, Duration};

/// A stream representing notifications at fixed interval
#[derive(Debug)]
pub struct Interval {
    sleep: Sleep,
    duration: Duration,
}

impl Interval {
    pub fn new(starting: Instant, duration: Duration) -> Interval {
        Interval {
            sleep: Sleep::new(starting),
            duration,
        }
    }
}

impl Stream for Interval {
    type Item = Instant;
    type Error = ::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // Wait for the sleep to be done
        let _ = try_ready!(self.sleep.poll());

        // Get the `now` by looking at the `sleep` deadline
        let now = self.sleep.deadline();

        // The next interval value is `duration` after the one that just
        // yielded.
        self.sleep = Sleep::new(now + self.duration);

        // Return the current instant
        Ok(Some(now).into())
    }
}
