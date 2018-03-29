//! Utilities for scheduling work to happen after a period of time.
//!
//! This crate provides a number of utilities for working with periods of time:
//!
//! * [`Sleep`]: A future that completes at a specified instant in time.
//!
//! * [`Interval`] A stream that yields at fixed time intervals.
//!
//! * [`Deadline`]: Wraps a future, requiring it to complete before a specified
//!   instant in time, erroring if the future takes too long.
//!
//! These three types are backed by a [`Timer`] instance. In order for
//! [`Sleep`], [`Interval`], and [`Deadline`] to function, the associated
//! [`Timer`] instance must be running on some thread.
//!
//! [`Sleep`]: struct.Sleep.html
//! [`Deadline`]: struct.Deadline.html
//! [`Interval`]: struct.Interval.html
//! [`Timer`]: timer/struct.Timer.html

#![doc(html_root_url = "https://docs.rs/tokio-timer/0.2.0")]
#![deny(missing_docs, warnings, missing_debug_implementations)]

extern crate tokio_executor;

#[macro_use]
extern crate futures;

pub mod timer;

mod atomic;
mod deadline;
mod error;
mod interval;
mod sleep;

pub use self::deadline::{Deadline, DeadlineError};
pub use self::error::Error;
pub use self::interval::Interval;
pub use self::timer::{Timer, with_default};
pub use self::sleep::Sleep;
