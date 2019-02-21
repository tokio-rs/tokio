#![allow(deprecated)]

use Delay;

use futures::{Async, Future, Poll};

use std::error;
use std::fmt;
use std::time::Instant;

#[deprecated(since = "0.2.6", note = "use Timeout instead")]
#[doc(hidden)]
#[derive(Debug)]
pub struct Deadline<T> {
    future: T,
    delay: Delay,
}

#[deprecated(since = "0.2.6", note = "use Timeout instead")]
#[doc(hidden)]
#[derive(Debug)]
pub struct DeadlineError<T>(Kind<T>);

/// Deadline error variants
#[derive(Debug)]
enum Kind<T> {
    /// Inner future returned an error
    Inner(T),

    /// The deadline elapsed.
    Elapsed,

    /// Timer returned an error.
    Timer(::Error),
}

impl<T> Deadline<T> {
    /// Create a new `Deadline` that completes when `future` completes or when
    /// `deadline` is reached.
    pub fn new(future: T, deadline: Instant) -> Deadline<T> {
        Deadline::new_with_delay(future, Delay::new(deadline))
    }

    pub(crate) fn new_with_delay(future: T, delay: Delay) -> Deadline<T> {
        Deadline { future, delay }
    }

    /// Gets a reference to the underlying future in this deadline.
    pub fn get_ref(&self) -> &T {
        &self.future
    }

    /// Gets a mutable reference to the underlying future in this deadline.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.future
    }

    /// Consumes this deadline, returning the underlying future.
    pub fn into_inner(self) -> T {
        self.future
    }
}

impl<T> Future for Deadline<T>
where
    T: Future,
{
    type Item = T::Item;
    type Error = DeadlineError<T::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // First, try polling the future
        match self.future.poll() {
            Ok(Async::Ready(v)) => return Ok(Async::Ready(v)),
            Ok(Async::NotReady) => {}
            Err(e) => return Err(DeadlineError::inner(e)),
        }

        // Now check the timer
        match self.delay.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => Err(DeadlineError::elapsed()),
            Err(e) => Err(DeadlineError::timer(e)),
        }
    }
}

// ===== impl DeadlineError =====

impl<T> DeadlineError<T> {
    /// Create a new `DeadlineError` representing the inner future completing
    /// with `Err`.
    pub fn inner(err: T) -> DeadlineError<T> {
        DeadlineError(Kind::Inner(err))
    }

    /// Returns `true` if the error was caused by the inner future completing
    /// with `Err`.
    pub fn is_inner(&self) -> bool {
        match self.0 {
            Kind::Inner(_) => true,
            _ => false,
        }
    }

    /// Consumes `self`, returning the inner future error.
    pub fn into_inner(self) -> Option<T> {
        match self.0 {
            Kind::Inner(err) => Some(err),
            _ => None,
        }
    }

    /// Create a new `DeadlineError` representing the inner future not
    /// completing before the deadline is reached.
    pub fn elapsed() -> DeadlineError<T> {
        DeadlineError(Kind::Elapsed)
    }

    /// Returns `true` if the error was caused by the inner future not
    /// completing before the deadline is reached.
    pub fn is_elapsed(&self) -> bool {
        match self.0 {
            Kind::Elapsed => true,
            _ => false,
        }
    }

    /// Creates a new `DeadlineError` representing an error encountered by the
    /// timer implementation
    pub fn timer(err: ::Error) -> DeadlineError<T> {
        DeadlineError(Kind::Timer(err))
    }

    /// Returns `true` if the error was caused by the timer.
    pub fn is_timer(&self) -> bool {
        match self.0 {
            Kind::Timer(_) => true,
            _ => false,
        }
    }

    /// Consumes `self`, returning the error raised by the timer implementation.
    pub fn into_timer(self) -> Option<::Error> {
        match self.0 {
            Kind::Timer(err) => Some(err),
            _ => None,
        }
    }
}

impl<T: error::Error> error::Error for DeadlineError<T> {
    fn description(&self) -> &str {
        use self::Kind::*;

        match self.0 {
            Inner(ref e) => e.description(),
            Elapsed => "deadline has elapsed",
            Timer(ref e) => e.description(),
        }
    }
}

impl<T: fmt::Display> fmt::Display for DeadlineError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use self::Kind::*;

        match self.0 {
            Inner(ref e) => e.fmt(fmt),
            Elapsed => "deadline has elapsed".fmt(fmt),
            Timer(ref e) => e.fmt(fmt),
        }
    }
}
