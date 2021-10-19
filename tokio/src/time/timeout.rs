//! Allows a future to execute for a maximum amount of time.
//!
//! See [`Timeout`] documentation for more details.
//!
//! [`Timeout`]: struct@Timeout

use crate::{
    time::{error::Elapsed, sleep_until, Duration, Instant, Sleep},
    util::trace,
};

use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{self, Poll};

/// Requires a `Future` to complete before the specified duration has elapsed.
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
#[cfg_attr(tokio_track_caller, track_caller)]
pub fn timeout<T>(duration: Duration, future: T) -> Timeout<T>
where
    T: Future,
{
    let location = trace::caller_location();

    let deadline = Instant::now().checked_add(duration);
    let delay = match deadline {
        Some(deadline) => Sleep::new_timeout(deadline, location),
        None => Sleep::far_future(location),
    };
    Timeout::new_with_delay(future, delay)
}

/// Requires a `Future` to complete before the specified instant in time.
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
    let delay = sleep_until(deadline);

    Timeout {
        value: future,
        delay,
    }
}

pin_project! {
    /// Future returned by [`timeout`](timeout) and [`timeout_at`](timeout_at).
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    #[derive(Debug)]
    pub struct Timeout<T> {
        #[pin]
        value: T,
        #[pin]
        delay: Sleep,
    }
}

impl<T> Timeout<T> {
    pub(crate) fn new_with_delay(value: T, delay: Sleep) -> Timeout<T> {
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

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let me = self.project();

        // First, try polling the future
        if let Poll::Ready(v) = me.value.poll(cx) {
            return Poll::Ready(Ok(v));
        }

        // Now check the timer
        match me.delay.poll(cx) {
            Poll::Ready(()) => Poll::Ready(Err(Elapsed::new())),
            Poll::Pending => Poll::Pending,
        }
    }
}
