use crate::time::driver::{Handle, TimerEntry};
use crate::time::{error::Error, Duration, Instant};

use std::future::Future;
use std::pin::Pin;

use std::task::{self, Poll};

/// Waits until `deadline` is reached.
///
/// No work is performed while awaiting on the sleep future to complete. `Sleep`
/// operates at millisecond granularity and should not be used for tasks that
/// require high-resolution timers.
///
/// # Cancellation
///
/// Canceling a sleep instance is done by dropping the returned future. No additional
/// cleanup work is required.
pub fn sleep_until(deadline: Instant) -> Sleep {
    Sleep::new_timeout(deadline)
}

/// Waits until `duration` has elapsed.
///
/// Equivalent to `sleep_until(Instant::now() + duration)`. An asynchronous
/// analog to `std::thread::sleep`.
///
/// No work is performed while awaiting on the sleep future to complete. `Sleep`
/// operates at millisecond granularity and should not be used for tasks that
/// require high-resolution timers.
///
/// To run something regularly on a schedule, see [`interval`].
///
/// The maximum duration for a sleep is 68719476734 milliseconds (approximately 2.2 years).
///
/// # Cancellation
///
/// Canceling a sleep instance is done by dropping the returned future. No additional
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
pub fn sleep(duration: Duration) -> Sleep {
    sleep_until(Instant::now() + duration)
}

/// Future returned by [`sleep`](sleep) and
/// [`sleep_until`](sleep_until).
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Sleep {
    deadline: Instant,

    // The link between the `Sleep` instance and the timer that drives it.
    // This will be unboxed in tokio 1.0
    entry: Pin<Box<TimerEntry>>,
}

impl Sleep {
    pub(crate) fn new_timeout(deadline: Instant) -> Sleep {
        let handle = Handle::current();
        let entry = Box::pin(TimerEntry::new(&handle, deadline));

        Sleep { deadline, entry }
    }

    /// Returns the instant at which the future will complete.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Returns `true` if `Sleep` has elapsed.
    ///
    /// A `Sleep` instance is elapsed when the requested duration has elapsed.
    pub fn is_elapsed(&self) -> bool {
        self.entry.is_elapsed()
    }

    /// Resets the `Sleep` instance to a new deadline.
    ///
    /// Calling this function allows changing the instant at which the `Sleep`
    /// future completes without having to create new associated state.
    ///
    /// This function can be called both before and after the future has
    /// completed.
    pub fn reset(&mut self, deadline: Instant) {
        self.entry.as_mut().reset(deadline);
        self.deadline = deadline;
    }

    fn poll_elapsed(&mut self, cx: &mut task::Context<'_>) -> Poll<Result<(), Error>> {
        // Keep track of task budget
        let coop = ready!(crate::coop::poll_proceed(cx));

        self.entry.as_mut().poll_elapsed(cx).map(move |r| {
            coop.made_progress();
            r
        })
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        // `poll_elapsed` can return an error in two cases:
        //
        // - AtCapacity: this is a pathological case where far too many
        //   sleep instances have been scheduled.
        // - Shutdown: No timer has been setup, which is a mis-use error.
        //
        // Both cases are extremely rare, and pretty accurately fit into
        // "logic errors", so we just panic in this case. A user couldn't
        // really do much better if we passed the error onwards.
        match ready!(self.as_mut().poll_elapsed(cx)) {
            Ok(()) => Poll::Ready(()),
            Err(e) => panic!("timer error: {}", e),
        }
    }
}
