//! Slow down a stream by enforcing a delay between items.

use {Delay, Error};

use futures::{Async, Future, Poll, Stream};
use futures::future::Either;

use std::time::{Duration, Instant};

/// Slow down a stream by enforcing a delay between items.
#[derive(Debug)]
pub struct Throttle<T> {
    delay: Option<Delay>,
    duration: Duration,
    stream: T,
}

/// Either the error of the underlying stream, or an error within
/// tokio's timing machinery.
#[derive(Debug)]
pub struct ThrottleError<T>(Either<T, Error>);

impl<T> Throttle<T> {
    /// Slow down a stream by enforcing a delay between items.
    pub fn new(stream: T, duration: Duration) -> Self {
        Self {
            delay: None,
            duration: duration,
            stream: stream,
        }
    }
}

impl<T: Stream> Stream for Throttle<T> {
    type Item = T::Item;
    type Error = ThrottleError<T::Error>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if let Some(ref mut delay) = self.delay {
            try_ready!(delay.poll().map_err(Either::B));
        }

        self.delay = None;
        let value = try_ready!(self.stream.poll().map_err(Either::A));

        if value.is_some() {
            self.delay = Some(Delay::new(Instant::now() + self.duration));
        }

        Ok(Async::Ready(value))
    }
}

impl<T> From<Either<T, Error>> for ThrottleError<T> {
    fn from(a_or_b: Either<T, Error>) -> Self {
        ThrottleError(a_or_b)
    }
}
