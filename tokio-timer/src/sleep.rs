use Error;
use timer::Registration;

use futures::{Future, Poll};

use std::time::Instant;

/// A future that completes at a specified instant in time.
///
/// This future performs no work.
#[derive(Debug)]
pub struct Sleep {
    deadline: Instant,
    registration: Option<Registration>,
}

// ===== impl Sleep =====

impl Sleep {
    /// Create a new `Sleep` instance that elapses at `deadline`.
    ///
    /// Only millisecond level resolution is guaranteed. There is no guarantee
    /// as to how the sub-millisecond portion of `deadline` will be handled.
    /// `Sleep` should not be used for high-resolution timer use cases.
    pub fn new(deadline: Instant) -> Sleep {
        Sleep {
            deadline,
            registration: None,
        }
    }

    pub(crate) fn new_with_registration(
        deadline: Instant,
        registration: Registration) -> Sleep
    {
        Sleep {
            deadline,
            registration: Some(registration),
        }
    }

    /// Returns the `Sleep` instance's deadline
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Returns true if the `Sleep` has elapsed
    ///
    /// A `Sleep` is elapsed when the requested duration has elapsed.
    pub fn is_elapsed(&self) -> bool {
        self.registration.as_ref()
            .map(|r| r.is_elapsed())
            .unwrap_or(false)
    }

    /// Reset the `Sleep` instance to a new deadline.
    pub fn reset(&mut self, deadline: Instant) {
        self.deadline = deadline;

        if let Some(registration) = self.registration.as_ref() {
            registration.reset(deadline);
        }
    }

    /// Register the sleep with the timer instance for the current execution
    /// context.
    fn register(&mut self) {
        if self.registration.is_some() {
            return;
        }

        self.registration = Some(Registration::new(self.deadline));
    }
}

impl Future for Sleep {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // Ensure the `Sleep` instance is associated with a timer.
        self.register();

        self.registration.as_ref().unwrap()
            .poll_elapsed()
    }
}
