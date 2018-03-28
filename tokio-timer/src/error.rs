use self::Kind::*;

use std::error;
use std::fmt;

/// Errors encountered by the timer implementation.
#[derive(Debug)]
pub struct Error(Kind);

#[derive(Debug)]
enum Kind {
    Shutdown,
    AtCapacity,
}

impl Error {
    /// Create an error representing a shutdown timer.
    pub fn shutdown() -> Error {
        Error(Shutdown)
    }

    /// Returns `true` if the error was caused by the timer being shutdown.
    pub fn is_shutdown(&self) -> bool {
        match self.0 {
            Kind::Shutdown => true,
            _ => false,
        }
    }

    /// Create an error representing a timer at capacity.
    pub fn at_capacity() -> Error {
        Error(AtCapacity)
    }

    /// Returns `true` if the error was caused by the timer being at capacity.
    pub fn is_at_capacity(&self) -> bool {
        match self.0 {
            Kind::AtCapacity => true,
            _ => false,
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        use self::Kind::*;

        match self.0 {
            Shutdown => "timer is shutdown",
            AtCapacity => "timer is at capacity and cannot create a new entry",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use std::error::Error;
        self.description().fmt(fmt)
    }
}
