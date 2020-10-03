use crate::time::driver::{Entry, Handle};
use crate::time::{Duration, Error, Instant};

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{self, Poll};

/// Waits until `deadline` is reached.
///
/// No work is performed while awaiting on the delay to complete. The delay
/// operates at millisecond granularity and should not be used for tasks that
/// require high-resolution timers.
///
/// # Cancellation
///
/// Canceling a delay is done by dropping the returned future. No additional
/// cleanup work is required.
pub fn sleep_until(deadline: Instant) -> Delay {
    Delay::new_timeout(deadline, Duration::from_millis(0))
}

/// Waits until `duration` has elapsed.
///
/// Equivalent to `sleep_until(Instant::now() + duration)`. An asynchronous
/// analog to `std::thread::sleep`.
///
/// No work is performed while awaiting on the delay to complete. The delay
/// operates at millisecond granularity and should not be used for tasks that
/// require high-resolution timers.
///
/// To run something regularly on a schedule, see [`interval`].
///
/// # Cancellation
///
/// Canceling a delay is done by dropping the returned future. No additional
/// cleanup work is required.
///
/// # Examples
///
/// Wait 100ms and print "100 ms have elapsed".
///
/// ```
/// use tokio::time::{sleep, Duration};
///
/// #[tokio::main]
/// async fn main() {
///     sleep(Duration::from_millis(100)).await;
///     println!("100 ms have elapsed");
/// }
/// ```
///
/// [`interval`]: crate::time::interval()
pub fn sleep(duration: Duration) -> Delay {
    sleep_until(Instant::now() + duration)
}

/// Future returned by [`sleep`](sleep) and
/// [`sleep_until`](sleep_until).
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Delay {
    /// The link between the `Delay` instance and the timer that drives it.
    ///
    /// This also stores the `deadline` value.
    entry: Arc<Entry>,
}

impl Delay {
    pub(crate) fn new_timeout(deadline: Instant, duration: Duration) -> Delay {
        let handle = Handle::current();
        let entry = Entry::new(&handle, deadline, duration);

        Delay { entry }
    }

    /// Returns the instant at which the future will complete.
    pub fn deadline(&self) -> Instant {
        self.entry.time_ref().deadline
    }

    /// Returns `true` if the `Delay` has elapsed
    ///
    /// A `Delay` is elapsed when the requested duration has elapsed.
    pub fn is_elapsed(&self) -> bool {
        self.entry.is_elapsed()
    }

    /// Resets the `Delay` instance to a new deadline.
    ///
    /// Calling this function allows changing the instant at which the `Delay`
    /// future completes without having to create new associated state.
    ///
    /// This function can be called both before and after the future has
    /// completed.
    pub fn reset(&mut self, deadline: Instant) {
        unsafe {
            self.entry.time_mut().deadline = deadline;
        }

        Entry::reset(&mut self.entry);
    }

    fn poll_elapsed(&self, cx: &mut task::Context<'_>) -> Poll<Result<(), Error>> {
        // Keep track of task budget
        let coop = ready!(crate::coop::poll_proceed(cx));

        self.entry.poll_elapsed(cx).map(move |r| {
            coop.made_progress();
            r
        })
    }
}

impl Future for Delay {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        // `poll_elapsed` can return an error in two cases:
        //
        // - AtCapacity: this is a pathological case where far too many
        //   delays have been scheduled.
        // - Shutdown: No timer has been setup, which is a mis-use error.
        //
        // Both cases are extremely rare, and pretty accurately fit into
        // "logic errors", so we just panic in this case. A user couldn't
        // really do much better if we passed the error onwards.
        match ready!(self.poll_elapsed(cx)) {
            Ok(()) => Poll::Ready(()),
            Err(e) => panic!("timer error: {}", e),
        }
    }
}

impl Drop for Delay {
    fn drop(&mut self) {
        Entry::cancel(&self.entry);
    }
}
