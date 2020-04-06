//! Allows a future to execute for a maximum amount of time.
//!
//! See [`Timeout`] documentation for more details.
//!
//! [`Timeout`]: struct@Timeout

use crate::time::{delay_until, Delay, Duration, Instant};

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{self, Poll};

/// Require a `Future` to complete before the specified duration has elapsed.
///
/// If the future completes before the duration has elapsed, then the completed
/// value is returned. Otherwise, an error is returned and the future is
/// canceled.
///
/// # Cancelation
///
/// Cancelling a timeout is done by dropping the future. No additional cleanup
/// or other work is required.
///
/// The original future may be obtained by calling [`Timeout::into_inner`]. This
/// consumes the `Timeout`.
///
/// # Examples
///
/// Create a new `Timeout` set to expire in 10 milliseconds.
///
/// ```rust
/// use tokio::time::timeout;
/// use tokio::sync::oneshot;
///
/// use std::time::Duration;
///
/// # async fn dox() {
/// let (tx, rx) = oneshot::channel();
/// # tx.send(()).unwrap();
///
/// // Wrap the future with a `Timeout` set to expire in 10 milliseconds.
/// if let Err(_) = timeout(Duration::from_millis(10), rx).await {
///     println!("did not receive value within 10 ms");
/// }
/// # }
/// ```
pub fn timeout<T>(duration: Duration, future: T) -> Timeout<T>
where
    T: Future,
{
    let delay = Delay::new_timeout(Instant::now() + duration, duration);
    Timeout::new_with_delay(future, delay)
}

/// Require a `Future` to complete before the specified instant in time.
///
/// If the future completes before the instant is reached, then the completed
/// value is returned. Otherwise, an error is returned.
///
/// # Cancelation
///
/// Cancelling a timeout is done by dropping the future. No additional cleanup
/// or other work is required.
///
/// The original future may be obtained by calling [`Timeout::into_inner`]. This
/// consumes the `Timeout`.
///
/// # Examples
///
/// Create a new `Timeout` set to expire in 10 milliseconds.
///
/// ```rust
/// use tokio::time::{Instant, timeout_at};
/// use tokio::sync::oneshot;
///
/// use std::time::Duration;
///
/// # async fn dox() {
/// let (tx, rx) = oneshot::channel();
/// # tx.send(()).unwrap();
///
/// // Wrap the future with a `Timeout` set to expire 10 milliseconds into the
/// // future.
/// if let Err(_) = timeout_at(Instant::now() + Duration::from_millis(10), rx).await {
///     println!("did not receive value within 10 ms");
/// }
/// # }
/// ```
pub fn timeout_at<T>(deadline: Instant, future: T) -> Timeout<T>
where
    T: Future,
{
    let delay = delay_until(deadline);

    Timeout {
        value: future,
        delay,
    }
}

/// Future returned by [`timeout`](timeout) and [`timeout_at`](timeout_at).
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[derive(Debug)]
pub struct Timeout<T> {
    value: T,
    delay: Delay,
}

/// Error returned by `Timeout`.
#[derive(Debug, PartialEq)]
pub struct Elapsed(());

impl Elapsed {
    // Used on StreamExt::timeout
    #[allow(unused)]
    pub(crate) fn new() -> Self {
        Elapsed(())
    }
}

impl<T> Timeout<T> {
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
