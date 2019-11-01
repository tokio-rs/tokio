use worker::Worker;

use futures::Poll;
use tokio_executor;

use std::error::Error;
use std::fmt;

mod global;

pub use self::global::blocking;
#[doc(hidden)]
pub use self::global::with_default;

/// Error raised by `blocking`.
pub struct BlockingError {
    _p: (),
}

pub trait Blocking {
    fn enter_blocking(&self) -> Poll<(), BlockingError>;
    fn exit_blocking(&self);
}

struct DefaultBlocking;
static DEFAULT_BLOCKING: DefaultBlocking = DefaultBlocking;

impl Blocking for DefaultBlocking {
    fn enter_blocking(&self) -> Poll<(), BlockingError> {
        Worker::with_current(|worker| {
            let worker = match worker {
                Some(worker) => worker,
                None => {
                    return Err(BlockingError { _p: () });
                }
            };

            // Transition the worker state to blocking. This will exit the fn early
            // with `NotReady` if the pool does not have enough capacity to enter
            // blocking mode.
            worker.transition_to_blocking()
        })
    }

    fn exit_blocking(&self) {
        // Try to transition out of blocking mode. This is a fast path that takes
        // back ownership of the worker if the worker handoff didn't complete yet.
        Worker::with_current(|worker| {
            // Worker must be set since it was above.
            worker.unwrap().transition_from_blocking();
        });
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
