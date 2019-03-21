use std::error::Error;
use std::fmt;

/// Errors returned by `Executor::spawn`.
///
/// Spawn errors should represent relatively rare scenarios. Currently, the two
/// scenarios represented by `SpawnError` are:
///
/// * An executor being at capacity or full. As such, the executor is not able
///   to accept a new future. This error state is expected to be transient.
/// * An executor has been shutdown and can no longer accept new futures. This
///   error state is expected to be permanent.
#[derive(Debug)]
pub struct SpawnError {
    is_shutdown: bool,
}

impl SpawnError {
    /// Return a new `SpawnError` reflecting a shutdown executor failure.
    pub fn shutdown() -> Self {
        SpawnError { is_shutdown: true }
    }

    /// Return a new `SpawnError` reflecting an executor at capacity failure.
    pub fn at_capacity() -> Self {
        SpawnError { is_shutdown: false }
    }

    /// Returns `true` if the error reflects a shutdown executor failure.
    pub fn is_shutdown(&self) -> bool {
        self.is_shutdown
    }

    /// Returns `true` if the error reflects an executor at capacity failure.
    pub fn is_at_capacity(&self) -> bool {
        !self.is_shutdown
    }
}

impl fmt::Display for SpawnError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.description())
    }
}

impl Error for SpawnError {
    fn description(&self) -> &str {
        "attempted to spawn task while the executor is at capacity or shut down"
    }
}
