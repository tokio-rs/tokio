use worker::Worker;

use futures::{Async, Poll};
use tokio_executor;

use std::error::Error;
use std::fmt;

mod global;
pub use self::global::blocking;
#[doc(hidden)]
pub use self::global::{set_default, with_default, DefaultGuard};

/// Error raised by `blocking`.
pub struct BlockingError {
    _p: (),
}

/// A function implementing the behavior run on calls to `blocking`.
///
/// **NOTE:** This is intended specifically for use by `tokio` 0.2's
/// backwards-compatibility layer. In general, user code should not override the
/// blocking implementation. If you use this, make sure you know what you're
/// doing.
#[doc(hidden)]
pub type BlockingImpl = fn(&mut dyn FnMut()) -> Poll<(), BlockingError>;

fn default_blocking(f: &mut dyn FnMut()) -> Poll<(), BlockingError> {
    let res = Worker::with_current(|worker| {
        let worker = match worker {
            Some(worker) => worker,
            None => {
                return Err(BlockingError::new());
            }
        };

        // Transition the worker state to blocking. This will exit the fn early
        // with `NotReady` if the pool does not have enough capacity to enter
        // blocking mode.
        worker.transition_to_blocking()
    });

    // If the transition cannot happen, exit early
    try_ready!(res);

    // Currently in blocking mode, so call the inner closure.
    //
    // "Exit" the current executor in case the blocking function wants
    // to call a different executor.
    tokio_executor::exit(move || (f)());

    // Try to transition out of blocking mode. This is a fast path that takes
    // back ownership of the worker if the worker handoff didn't complete yet.
    Worker::with_current(|worker| {
        // Worker must be set since it was above.
        worker.unwrap().transition_from_blocking();
    });

    Ok(Async::Ready(()))
}

impl BlockingError {
    /// Returns a new `BlockingError`.
    #[doc(hidden)]
    pub fn new() -> Self {
        Self { _p: () }
    }
}

impl fmt::Display for BlockingError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.description())
    }
}

impl fmt::Debug for BlockingError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BlockingError")
            .field("reason", &self.description())
            .finish()
    }
}

impl Error for BlockingError {
    fn description(&self) -> &str {
        "`blocking` annotation used from outside the context of a thread pool"
    }
}
