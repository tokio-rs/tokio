use tokio_current_thread as current_thread;

use std::error::Error;
use std::fmt;


/// Error returned by the `run` function.
#[derive(Debug)]
pub struct RunError {
    pub(crate) inner: current_thread::RunError,
}

impl fmt::Display for RunError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.description())
    }
}

impl Error for RunError {
    fn description(&self) -> &str {
        "An error occured in current_thread::Runtime::run"
    }
}
