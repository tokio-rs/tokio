//! Allows a future or stream to execute for a maximum amount of time.
//!
//! See [`Timeout`] documentation for more details.
//!
//! [`Timeout`]: struct.Timeout.html

use Delay;
use clock::now;

use futures::{Future, Stream, Poll, Async};

use std::error;
use std::fmt;
use std::time::{Instant, Duration};

/// Allows a `Future` or `Stream` to execute for a limited amount of time.
///
/// If thee future or stream completes before the timeout has expired, then
/// `Timeout` returns the completed value. Otherwise, `Timeout` returns an
/// [`Error`].
///
/// # Futures and Streams
///
/// The exact behavor depends on if the inner value is a `Future` or a `Stream`.
/// In the case of a `Future`, `Timeout` will require the future to complete by
/// a fixed deadline. In the case of a `Stream`, `Timeout` will allow each item
/// to take the entire timeout before returning an error.
///
/// In order to set an upper bound on the processing of the *entire* stream,
/// then a timeout should be set on the future that processes the stream. For
/// example:
///
/// ```rust
/// # extern crate futures;
/// # extern crate tokio;
/// use tokio::timer::Timeout;
/// use futures::{Future, Stream};
/// use futures::sync::mpsc;
/// use std::time::Duration;
///
/// # fn main() {
/// let (tx, rx) = mpsc::unbounded();
/// # tx.unbounded_send(()).unwrap();
/// # drop(tx);
/// let process = rx.for_each(|item| {
///     // do something with `iteem`
/// # drop(item);
/// # Ok(())
/// });
///
/// Timeout::new(process, Duration::from_secs(1));
/// # }
/// ```
///
/// # Cancelation
///
/// Cancelling a `Timeout` is done by dropping the value. No additional cleanup
/// or otheer work is required.
///
/// The original future or stream may be obtained by calling [`into_inner`]. This
/// consumes the `Timeout`.
///
/// [`Error`]: struct.Error.html
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Timeout<T> {
    value: T,
    delay: Delay,
}

/// Error returned by `Timeout`.
#[derive(Debug)]
pub struct Error<T>(Kind<T>);

/// Timeout error variants
#[derive(Debug)]
enum Kind<T> {
    /// Inner value returned an error
    Inner(T),

    /// The timeout elapsed.
    Elapsed,

    /// Timer returned an error.
    Timer(::Error),
}

impl<T> Timeout<T> {
    /// Create a new `Timeout` that allows `value` to execute for a duration of
    /// at most `timeout`.
    ///
    /// The exact behavior depends on if `value` is a `Future` or a `Stream`.
    ///
    /// See [type] level documentation for more details.
    ///
    /// [type]: #
    pub fn new(value: T, timeout: Duration) -> Timeout<T> {
        let delay = Delay::new_timeout(now() + timeout, timeout);

        Timeout {
            value,
            delay,
        }
    }

    /// Gets a reference to the underlying value in this timeout.
    pub fn get_ref(&self) -> &T {
        &self.value
    }

    /// Gets a mutable reference to the underlying value in this timeout.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Consumes this timeout, returning the underlying value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T: Future> Timeout<T> {
    /// Create a new `Timeout` that completes when `future` completes or when
    /// `deadline` is reached.
    ///
    /// This function differs from `new` in that:
    ///
    /// * It only accepts `Future` arguments.
    /// * It sets an explicit `Instant` at which the timeout expires.
    pub fn new_at(future: T, deadline: Instant) -> Timeout<T> {
        let delay = Delay::new(deadline);

        Timeout {
            value: future,
            delay,
        }
    }
}

impl<T> Future for Timeout<T>
where T: Future,
{
    type Item = T::Item;
    type Error = Error<T::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // First, try polling the future
        match self.value.poll() {
            Ok(Async::Ready(v)) => return Ok(Async::Ready(v)),
            Ok(Async::NotReady) => {}
            Err(e) => return Err(Error::inner(e)),
        }

        // Now check the timer
        match self.delay.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => {
                Err(Error::elapsed())
            },
            Err(e) => Err(Error::timer(e)),
        }
    }
}

impl<T> Stream for Timeout<T>
where T: Stream,
{
    type Item = T::Item;
    type Error = Error<T::Error>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // First, try polling the future
        match self.value.poll() {
            Ok(Async::Ready(v)) => {
                if v.is_some() {
                    self.delay.reset_timeout();
                }
                return Ok(Async::Ready(v))
            }
            Ok(Async::NotReady) => {}
            Err(e) => return Err(Error::inner(e)),
        }

        // Now check the timer
        match self.delay.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => {
                Err(Error::elapsed())
            },
            Err(e) => Err(Error::timer(e)),
        }
    }
}

// ===== impl Error =====

impl<T> Error<T> {
    /// Create a new `Error` representing the inner value completing with `Err`.
    pub fn inner(err: T) -> Error<T> {
        Error(Kind::Inner(err))
    }

    /// Returns `true` if the error was caused by the inner value completing
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

    /// Create a new `Error` representing the inner value not completing before
    /// the deadline is reached.
    pub fn elapsed() -> Error<T> {
        Error(Kind::Elapsed)
    }

    /// Returns `true` if the error was caused by the inner value not completing
    /// before the deadline is reached.
    pub fn is_elapsed(&self) -> bool {
        match self.0 {
            Kind::Elapsed => true,
            _ => false,
        }
    }

    /// Creates a new `Error` representing an error encountered by the timer
    /// implementation
    pub fn timer(err: ::Error) -> Error<T> {
        Error(Kind::Timer(err))
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

impl<T: error::Error> error::Error for Error<T> {
    fn description(&self) -> &str {
        use self::Kind::*;

        match self.0 {
            Inner(ref e) => e.description(),
            Elapsed => "deadline has elapsed",
            Timer(ref e) => e.description(),
        }
    }
}

impl<T: fmt::Display> fmt::Display for Error<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use self::Kind::*;

        match self.0 {
            Inner(ref e) => e.fmt(fmt),
            Elapsed => "deadline has elapsed".fmt(fmt),
            Timer(ref e) => e.fmt(fmt),
        }
    }
}
