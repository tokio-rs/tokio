use self::Kind::*;
use std::error;
use std::fmt;

/// Errors encountered by the timer implementation.
///
/// Currently, there are two different errors that can occur:
///
/// * `shutdown` occurs when a timer operation is attempted, but the timer
///   instance has been dropped. In this case, the operation will never be able
///   to complete and the `shutdown` error is returned. This is a permanent
///   error, i.e., once this error is observed, timer operations will never
///   succeed in the future.
///
/// * `at_capacity` occurs when a timer operation is attempted, but the timer
///   instance is currently handling its maximum number of outstanding delays.
///   In this case, the operation is not able to be performed at the current
///   moment, and `at_capacity` is returned. This is a transient error, i.e., at
///   some point in the future, if the operation is attempted again, it might
///   succeed. Callers that observe this error should attempt to [shed load]. One
///   way to do this would be dropping the future that issued the timer operation.
///
/// [shed load]: https://en.wikipedia.org/wiki/Load_Shedding
#[derive(Debug)]
pub struct Error(Kind);

#[derive(Debug)]
enum Kind {
    Shutdown,
    AtCapacity,
}

impl Error {
    /// Creates an error representing a shutdown timer.
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

    /// Creates an error representing a timer at capacity.
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

impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::Kind::*;
        let descr = match self.0 {
            Shutdown => "the timer is shutdown, must be called from the context of Tokio runtime",
            AtCapacity => "timer is at capacity and cannot create a new entry",
        };
        write!(fmt, "{}", descr)
    }
}
