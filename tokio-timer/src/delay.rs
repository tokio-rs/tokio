use Error;
use timer::Registration;

use futures::{Future, Poll};

use std::time::Instant;

/// A future that completes at a specified instant in time.
///
/// Instances of `Delay` perform no work and complete with `()` once the
/// specified deadline has been reached.
///
/// `Delay` has a resolution of one millisecond and should not be used for tasks
/// that require high-resolution timers.
///
/// [`new`]: #method.new
#[derive(Debug)]
pub struct Delay {
    /// The instant at which the future completes.
    deadline: Instant,

    /// The link between the `Delay` instance at the timer that drives it.
    ///
    /// When `Delay` is created with `new`, this is initialized to `None` and is
    /// lazily set in `poll`. When `poll` is called, the default for the current
    /// execution context is used (obtained via `Handle::current`).
    ///
    /// When `delay` is created with `new_with_registration`, the value is set.
    ///
    /// Once `registration` is set to `Some`, it is never changed.
    registration: Option<Registration>,
}

impl Delay {
    /// Create a new `Delay` instance that elapses at `deadline`.
    ///
    /// Only millisecond level resolution is guaranteed. There is no guarantee
    /// as to how the sub-millisecond portion of `deadline` will be handled.
    /// `Delay` should not be used for high-resolution timer use cases.
    pub fn new(deadline: Instant) -> Delay {
        Delay {
            deadline,
            registration: None,
        }
    }

    pub(crate) fn new_with_registration(
        deadline: Instant,
        registration: Registration) -> Delay
    {
        Delay {
            deadline,
            registration: Some(registration),
        }
    }

    /// Returns the instant at which the future will complete.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Returns true if the `Delay` has elapsed
    ///
    /// A `Delay` is elapsed when the requested duration has elapsed.
    pub fn is_elapsed(&self) -> bool {
        self.registration.as_ref()
            .map(|r| r.is_elapsed())
            .unwrap_or(false)
    }

    /// Reset the `Delay` instance to a new deadline.
    ///
    /// Calling this function allows changing the instant at which the `Delay`
    /// future completes without having to create new associated state.
    ///
    /// This function can be called both before and after the future has
    /// completed.
    pub fn reset(&mut self, deadline: Instant) {
        self.deadline = deadline;

        if let Some(registration) = self.registration.as_ref() {
            registration.reset(deadline);
        }
    }

    /// Register the delay with the timer instance for the current execution
    /// context.
    fn register(&mut self) {
        if self.registration.is_some() {
            return;
        }

        self.registration = Some(Registration::new(self.deadline));
    }
}

impl Future for Delay {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // Ensure the `Delay` instance is associated with a timer.
        self.register();

        self.registration.as_ref().unwrap()
            .poll_elapsed()
    }
}
