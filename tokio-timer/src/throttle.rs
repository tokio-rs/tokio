//! Slow down a stream by enforcing a delay between items.

use {clock, Delay, Error};

use futures::{Async, Future, Poll, Stream};
use futures::future::Either;

use std::time::Duration;

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
            try_ready!({
                delay.poll()
                    .map_err(ThrottleError::from_timer_err)
            });
        }

        self.delay = None;
        let value = try_ready!({
            self.stream.poll()
                .map_err(ThrottleError::from_stream_err)
        });

        if value.is_some() {
            self.delay = Some(Delay::new(clock::now() + self.duration));
        }

        Ok(Async::Ready(value))
    }
}

impl<T> ThrottleError<T> {
    /// Creates a new `ThrottleError` from the given stream error.
    pub fn from_stream_err(err: T) -> Self {
        ThrottleError(Either::A(err))
    }

    /// Creates a new `ThrottleError` from the given tokio timer error.
    pub fn from_timer_err(err: Error) -> Self {
        ThrottleError(Either::B(err))
    }

    /// Attempts to get the underlying stream error, if it is present.
    pub fn get_stream_error(&self) -> Option<&T> {
        match self.0 {
            Either::A(ref x) => Some(x),
            _ => None,
        }
    }

    /// Attempts to get the underlying timer error, if it is present.
    pub fn get_timer_error(&self) -> Option<&Error> {
        match self.0 {
            Either::B(ref x) => Some(x),
            _ => None,
        }
    }

    /// Attempts to extract the underlying stream error, if it is present.
    pub fn into_stream_error(self) -> Option<T> {
        match self.0 {
            Either::A(x) => Some(x),
            _ => None,
        }
    }

    /// Attempts to extract the underlying timer error, if it is present.
    pub fn into_timer_error(self) -> Option<Error> {
        match self.0 {
            Either::B(x) => Some(x),
            _ => None,
        }
    }

    /// Returns whether the throttle error has occured because of an error
    /// in the underlying stream.
    pub fn is_stream_error(&self) -> bool {
        !self.is_timer_error()
    }

    /// Returns whether the throttle error has occured because of an error
    /// in tokio's timer system.
    pub fn is_timer_error(&self) -> bool {
        match self.0 {
            Either::A(_) => false,
            Either::B(_) => true,
        }
    }
}
