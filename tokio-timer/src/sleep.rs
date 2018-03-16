use Error;
use timer::Registration;

use futures::{Future, Poll};

use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Sleep {
    deadline: Instant,
    registration: Option<Registration>,
}

// ===== impl Sleep =====

impl Sleep {
    /// Create a new `Sleep` instance that elapses after `duration`.
    pub fn new(duration: Duration) -> Sleep {
        Sleep::until(Instant::now() + duration)
    }

    /// Create a new `Sleep` instance that elapses at `deadline`.
    pub fn until(deadline: Instant) -> Sleep {
        Sleep {
            deadline,
            registration: None,
        }
    }

    /// Returns true if the `Sleep` has elapsed
    ///
    /// A `Sleep` is elapsed when the requested duration has elapsed.
    pub fn is_elapsed(&self) -> bool {
        self.registration.as_ref()
            .map(|r| r.is_elapsed())
            .unwrap_or(false)
    }

    /// Register the sleep with the timer instance for the current execution
    /// context.
    fn register(&mut self) -> Result<(), Error> {
        if self.registration.is_some() {
            return Ok(());
        }

        self.registration = Some(Registration::new(self.deadline)?);
        Ok(())
    }
}

impl Future for Sleep {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // Ensure the `Sleep` instance is associated with a timer.
        self.register()?;

        self.registration.as_ref().unwrap()
            .poll_elapsed()
    }
}
