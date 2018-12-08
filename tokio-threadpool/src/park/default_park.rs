use tokio_executor::park::{Park, Unpark};

use std::error::Error;
use std::fmt;
use std::time::Duration;

use crossbeam_utils::sync::{Parker, Unparker};

/// Parks the thread.
#[derive(Debug)]
pub struct DefaultPark {
    inner: Parker,
}

/// Unparks threads that were parked by `DefaultPark`.
#[derive(Debug)]
pub struct DefaultUnpark {
    inner: Unparker,
}

/// Error returned by [`ParkThread`]
///
/// This currently is never returned, but might at some point in the future.
///
/// [`ParkThread`]: struct.ParkThread.html
#[derive(Debug)]
pub struct ParkError {
    _p: (),
}

// ===== impl DefaultPark =====

impl DefaultPark {
    /// Creates a new `DefaultPark` instance.
    pub fn new() -> DefaultPark {
        DefaultPark {
            inner: Parker::new(),
        }
    }

    /// Unpark the thread without having to clone the unpark handle.
    ///
    /// Named `notify` to avoid conflicting with the `unpark` fn.
    pub(crate) fn notify(&self) {
        self.inner.unparker().unpark();
    }

    pub(crate) fn park_sync(&self, duration: Option<Duration>) {
        match duration {
            None => self.inner.park(),
            Some(duration) => self.inner.park_timeout(duration),
        }
    }
}

impl Park for DefaultPark {
    type Unpark = DefaultUnpark;
    type Error = ParkError;

    fn unpark(&self) -> Self::Unpark {
        DefaultUnpark {
            inner: self.inner.unparker().clone(),
        }
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.inner.park();
        Ok(())
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.inner.park_timeout(duration);
        Ok(())
    }
}

// ===== impl DefaultUnpark =====

impl Unpark for DefaultUnpark {
    fn unpark(&self) {
        self.inner.unpark();
    }
}

// ===== impl ParkError =====

impl fmt::Display for ParkError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.description().fmt(fmt)
    }
}

impl Error for ParkError {
    fn description(&self) -> &str {
        "unknown park error"
    }
}
