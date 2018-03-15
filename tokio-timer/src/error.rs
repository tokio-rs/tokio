use self::Kind::*;

/// Errors that occur when working with a timer.
pub struct Error(Kind);

enum Kind {
    Shutdown,
    AtCapacity,
}

impl Error {
    /// Create an error representing a shutdown timer.
    pub fn shutdown() -> Error {
        Error(Shutdown)
    }

    /// Create an error representing a timer at capacity.
    pub fn at_capacity() -> Error {
        Error(AtCapacity)
    }
}
