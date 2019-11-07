//! Allows a future or stream to execute for a maximum amount of time.
//!
//! See [`Timeout`] documentation for more details.
//!
//! [`Timeout`]: struct.Timeout.html

use crate::time::clock::now;
use crate::time::Delay;

use futures_core::ready;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{self, Poll};
use std::time::{Duration, Instant};

/// Allows a `Future` or `Stream` to execute for a limited amount of time.
///
/// If the future or stream completes before the timeout has expired, then
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
/// ```rust,no_run
/// use tokio::prelude::*;
/// use tokio::sync::mpsc;
///
/// use std::thread;
/// use std::time::Duration;
///
/// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
/// let (mut tx, rx) = mpsc::unbounded_channel();
///
/// thread::spawn(move || {
///     tx.try_send(()).unwrap();
///     thread::sleep(Duration::from_millis(10));
///     tx.try_send(()).unwrap();
/// });
///
/// let process = rx.for_each(|item| {
///     // do something with `item`
/// # drop(item);
/// # tokio::future::ready(())
/// });
///
/// // Wrap the future with a `Timeout` set to expire in 10 milliseconds.
/// process.timeout(Duration::from_millis(10)).await?;
/// # Ok(())
/// # }
/// ```
///
/// # Cancelation
///
/// Cancelling a `Timeout` is done by dropping the value. No additional cleanup
/// or other work is required.
///
/// The original future or stream may be obtained by calling [`Timeout::into_inner`]. This
/// consumes the `Timeout`.
///
/// [`Error`]: struct.Error.html
/// [`Timeout::into_inner`]: struct.Timeout.html#method.into_iter
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[derive(Debug)]
pub struct Timeout<T> {
    value: T,
    delay: Delay,
}

/// Error returned by `Timeout`.
#[derive(Debug)]
pub struct Elapsed(());

impl<T> Timeout<T> {
    /// Create a new `Timeout` that allows `value` to execute for a duration of
    /// at most `timeout`.
    ///
    /// The exact behavior depends on if `value` is a `Future` or a `Stream`.
    ///
    /// See [type] level documentation for more details.
    ///
    /// [type]: #
    ///
    /// # Examples
    ///
    /// Create a new `Timeout` set to expire in 10 milliseconds.
    ///
    /// ```rust
    /// use tokio::time::Timeout;
    /// use tokio::sync::oneshot;
    ///
    /// use std::time::Duration;
    ///
    /// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
    /// let (tx, rx) = oneshot::channel();
    /// # tx.send(()).unwrap();
    ///
    /// // Wrap the future with a `Timeout` set to expire in 10 milliseconds.
    /// Timeout::new(rx, Duration::from_millis(10)).await??;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(value: T, timeout: Duration) -> Timeout<T> {
        let delay = Delay::new_timeout(now() + timeout, timeout);
        Timeout::new_with_delay(value, delay)
    }

    pub(crate) fn new_with_delay(value: T, delay: Delay) -> Timeout<T> {
        Timeout { value, delay }
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
where
    T: Future,
{
    type Output = Result<T::Output, Elapsed>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        // First, try polling the future

        // Safety: we never move `self.value`
        unsafe {
            let p = self.as_mut().map_unchecked_mut(|me| &mut me.value);
            if let Poll::Ready(v) = p.poll(cx) {
                return Poll::Ready(Ok(v));
            }
        }

        // Now check the timer
        // Safety: X_X!
        unsafe {
            match self.map_unchecked_mut(|me| &mut me.delay).poll(cx) {
                Poll::Ready(()) => Poll::Ready(Err(Elapsed(()))),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}

impl<T> futures_core::Stream for Timeout<T>
where
    T: futures_core::Stream,
{
    type Item = Result<T::Item, Elapsed>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        // Safety: T might be !Unpin, but we never move neither `value`
        // nor `delay`.
        //
        // ... X_X
        unsafe {
            // First, try polling the future
            let v = self
                .as_mut()
                .map_unchecked_mut(|me| &mut me.value)
                .poll_next(cx);

            if let Poll::Ready(v) = v {
                if v.is_some() {
                    self.as_mut().get_unchecked_mut().delay.reset_timeout();
                }
                return Poll::Ready(v.map(Ok));
            }

            // Now check the timer
            ready!(self.as_mut().map_unchecked_mut(|me| &mut me.delay).poll(cx));

            // if delay was ready, timeout elapsed!
            self.as_mut().get_unchecked_mut().delay.reset_timeout();
            Poll::Ready(Some(Err(Elapsed(()))))
        }
    }
}

// ===== impl Elapsed =====

impl fmt::Display for Elapsed {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        "deadline has elapsed".fmt(fmt)
    }
}

impl std::error::Error for Elapsed {}

impl From<Elapsed> for std::io::Error {
    fn from(_err: Elapsed) -> std::io::Error {
        std::io::ErrorKind::TimedOut.into()
    }
}
