//! Slow down a stream by enforcing a delay between items.

use {clock, Delay, Error};

use futures::future::Either;
use futures::{Async, Future, Poll, Stream};

use std::{
    error::Error as StdError,
    fmt::{Display, Formatter, Result as FmtResult},
    time::Duration,
};

/// Slow down a stream by enforcing a delay between items.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
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
    type Error = ThrottleError<T::Error>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if let Some(ref mut delay) = self.delay {
            try_ready!({ delay.poll().map_err(ThrottleError::from_timer_err) });
        }

        self.delay = None;
        let value = try_ready!({ self.stream.poll().map_err(ThrottleError::from_stream_err) });

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

impl<T: StdError> Display for ThrottleError<T> {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self.0 {
            Either::A(ref err) => write!(f, "stream error: {}", err),
            Either::B(ref err) => write!(f, "timer error: {}", err),
        }
    }
}

impl<T: StdError + 'static> StdError for ThrottleError<T> {
    fn description(&self) -> &str {
        match self.0 {
            Either::A(_) => "stream error",
            Either::B(_) => "timer error",
        }
    }

    // FIXME(taiki-e): When the minimum support version of tokio reaches Rust 1.30,
    // replace this with Error::source.
    #[allow(deprecated)]
    fn cause(&self) -> Option<&dyn StdError> {
        match self.0 {
            Either::A(ref err) => Some(err),
            Either::B(ref err) => Some(err),
        }
    }
}
