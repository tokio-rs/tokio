extern crate tokio_executor;

#[macro_use]
extern crate futures;

pub mod timer;

mod atomic;
mod deadline;
mod error;
mod handle;
mod interval;
mod now;
mod sleep;

pub use self::deadline::{Deadline, DeadlineError};
pub use self::error::Error;
pub use self::interval::Interval;
pub use self::timer::{Timer, with_default};
pub use self::sleep::Sleep;
