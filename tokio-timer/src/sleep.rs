use Error;
use timer::{Handle, Registration};

use futures::{Future, Async, Poll};

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
        unimplemented!();
    }

    /// Create a new `Sleep` instance that elapses at `deadline`.
    pub fn until(deadline: Instant) -> Sleep {
        unimplemented!();
    }

    /// Returns true if the `Sleep` is expired.
    ///
    /// A `Sleep` is expired when the requested duration has elapsed.
    pub fn is_expired(&self) -> bool {
        unimplemented!();
    }

    /// Returns the duration remaining
    pub fn remaining(&self) -> Duration {
        let now = Instant::now();

        if now >= self.deadline {
            Duration::from_millis(0)
        } else {
            self.deadline - now
        }
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

        unimplemented!();
    }
}
