use crate::time::driver::Registration;
use crate::time::{Duration, Instant};

use std::future::Future;
use std::pin::Pin;
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
pub fn delay_until(deadline: Instant) -> Delay {
    let registration = Registration::new(deadline, Duration::from_millis(0));
    Delay { registration }
}

/// Waits until `duration` has elapsed.
///
/// Equivalent to `delay_until(Instant::now() + duration)`. An asynchronous
/// analog to `std::thread::sleep`.
///
/// No work is performed while awaiting on the delay to complete. The delay
/// operates at millisecond granularity and should not be used for tasks that
/// require high-resolution timers.
///
/// # Cancellation
///
/// Canceling a delay is done by dropping the returned future. No additional
/// cleanup work is required.
pub fn delay_for(duration: Duration) -> Delay {
    delay_until(Instant::now() + duration)
}

/// Future returned by [`delay_until`](delay_until) and
/// [`delay_for`](delay_for).
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Delay {
    /// The link between the `Delay` instance and the timer that drives it.
    ///
    /// This also stores the `deadline` value.
    registration: Registration,
}

impl Delay {
    pub(crate) fn new_timeout(deadline: Instant, duration: Duration) -> Delay {
        let registration = Registration::new(deadline, duration);
        Delay { registration }
    }

    /// Returns the instant at which the future will complete.
    pub fn deadline(&self) -> Instant {
        self.registration.deadline()
    }

    /// Returns `true` if the `Delay` has elapsed
    ///
    /// A `Delay` is elapsed when the requested duration has elapsed.
    pub fn is_elapsed(&self) -> bool {
        self.registration.is_elapsed()
    }

    /// Resets the `Delay` instance to a new deadline.
    ///
    /// Calling this function allows changing the instant at which the `Delay`
    /// future completes without having to create new associated state.
    ///
    /// This function can be called both before and after the future has
    /// completed.
    pub fn reset(&mut self, deadline: Instant) {
        self.registration.reset(deadline);
    }
}

impl Future for Delay {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        // `poll_elapsed` can return an error in two cases:
        //
        // - AtCapacity: this is a pathlogical case where far too many
        //   delays have been scheduled.
        // - Shutdown: No timer has been setup, which is a mis-use error.
        //
        // Both cases are extremely rare, and pretty accurately fit into
        // "logic errors", so we just panic in this case. A user couldn't
        // really do much better if we passed the error onwards.
        match ready!(self.registration.poll_elapsed(cx)) {
            Ok(()) => Poll::Ready(()),
            Err(e) => panic!("timer error: {}", e),
        }
    }
}
