use Sleep;

use futures::{Stream, Poll};

use std::time::{Instant, Duration};

/// A stream representing notifications at fixed interval
#[derive(Debug)]
pub struct Interval {
    sleep: Sleep,
    duration: Duration,
}

impl Interval {
    pub fn new(starting: Instant, interval: Duration) -> Interval {
        unimplemented!();
    }
}

impl Stream for Interval {
    type Item = Instant;
    type Error = ::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        unimplemented!();
    }
}
