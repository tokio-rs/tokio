use timer::{HandlePriv, Registration};
use Error;

use futures::{Future, Poll};

use std::time::{Duration, Instant};

/// A future that completes at a specified instant in time.
///
/// Instances of `Delay` perform no work and complete with `()` once the
/// specified deadline has been reached.
///
/// `Delay` has a resolution of one millisecond and should not be used for tasks
/// that require high-resolution timers.
///
/// # Cancellation
///
/// Canceling a `Delay` is done by dropping the value. No additional cleanup or
/// other work is required.
///
/// [`new`]: #method.new
#[derive(Debug)]
pub struct Delay {
    /// The link between the `Delay` instance at the timer that drives it.
    ///
    /// This also stores the `deadline` value.
    registration: Registration,
}

impl Delay {
    /// Create a new `Delay` instance that elapses at `deadline`.
    ///
    /// Only millisecond level resolution is guaranteed. There is no guarantee
    /// as to how the sub-millisecond portion of `deadline` will be handled.
    /// `Delay` should not be used for high-resolution timer use cases.
    pub fn new(deadline: Instant) -> Delay {
        let registration = Registration::new(deadline, Duration::from_millis(0));

        Delay { registration }
    }

    pub(crate) fn new_timeout(deadline: Instant, duration: Duration) -> Delay {
        let registration = Registration::new(deadline, duration);
        Delay { registration }
    }

    pub(crate) fn new_with_handle(deadline: Instant, handle: HandlePriv) -> Delay {
        let mut registration = Registration::new(deadline, Duration::from_millis(0));
        registration.register_with(handle);

        Delay { registration }
    }

    /// Returns the instant at which the future will complete.
    pub fn deadline(&self) -> Instant {
        self.registration.deadline()
    }

    /// Returns true if the `Delay` has elapsed
    ///
    /// A `Delay` is elapsed when the requested duration has elapsed.
    pub fn is_elapsed(&self) -> bool {
        self.registration.is_elapsed()
    }

    /// Reset the `Delay` instance to a new deadline.
    ///
    /// Calling this function allows changing the instant at which the `Delay`
    /// future completes without having to create new associated state.
    ///
    /// This function can be called both before and after the future has
    /// completed.
    pub fn reset(&mut self, deadline: Instant) {
        self.registration.reset(deadline);
    }

    pub(crate) fn reset_timeout(&mut self) {
        self.registration.reset_timeout();
    }

    /// Register the delay with the timer instance for the current execution
    /// context.
    fn register(&mut self) {
        self.registration.register();
    }
}

impl Future for Delay {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // Ensure the `Delay` instance is associated with a timer.
        self.register();

        self.registration.poll_elapsed()
    }
}
