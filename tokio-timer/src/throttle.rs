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
    stream: Option<T>,
}

impl<T> Throttle<T> {
    /// Slow down a stream by enforcing a delay between items.
    pub fn new(stream: T, duration: Duration) -> Self {
        Self {
            delay: None,
            duration: duration,
            stream: Some(stream),
        }
    }
}

impl<T: Stream> Stream for Throttle<T> {
    type Item = T::Item;
    type Error = Either<T::Error, Error>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // 1. Is there a better way than nested match statements?
        // 2. What about having a delay and the underlying stream error'd?

        match self.delay.take() {
            Some(mut d) => match d.poll() {
                Ok(Async::Ready(_)) => self.poll(),
                Ok(Async::NotReady) => {
                    self.delay = Some(d);

                    Ok(Async::NotReady)
                },
                Err(e) => Err(Either::B(e)),
            },

            None => match self.stream.take() {
                Some(mut s) => match s.poll() {
                    Ok(Async::Ready(Some(it))) => {
                        self.delay = Some(Delay::new(Instant::now() + self.duration));
                        self.stream = Some(s);

                        Ok(Async::Ready(Some(it)))
                    },
                    Ok(Async::Ready(None)) => Ok(Async::Ready(None)),
                    Ok(Async::NotReady) => {
                        self.stream = Some(s);

                        Ok(Async::NotReady)
                    },
                    Err(e) => Err(Either::A(e)),
                },
                None => Ok(Async::Ready(None)),
            },
        }
    }
}
