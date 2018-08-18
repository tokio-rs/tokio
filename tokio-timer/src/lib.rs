//! Utilities for scheduling work to happen after a period of time.
//!
//! This crate provides a number of utilities for working with periods of time:
//!
//! * [`Delay`]: A future that completes at a specified instant in time.
//!
//! * [`Interval`] A stream that yields at fixed time intervals.
//!
//! * [`Deadline`]: Wraps a future, requiring it to complete before a specified
//!   instant in time, erroring if the future takes too long.
//!
//! These three types are backed by a [`Timer`] instance. In order for
//! [`Delay`], [`Interval`], and [`Deadline`] to function, the associated
//! [`Timer`] instance must be running on some thread.
//!
//! [`Delay`]: struct.Delay.html
//! [`Deadline`]: struct.Deadline.html
//! [`Interval`]: struct.Interval.html
//! [`Timer`]: timer/struct.Timer.html

#![doc(html_root_url = "https://docs.rs/tokio-timer/0.2.5")]
#![deny(missing_docs, warnings, missing_debug_implementations)]

extern crate tokio_executor;

#[macro_use]
extern crate futures;
extern crate slab;

pub mod clock;
pub mod delay_queue;
pub mod timer;

mod atomic;
mod deadline;
mod delay;
mod error;
mod interval;
mod wheel;

pub use self::deadline::{Deadline, DeadlineError};
pub use self::delay_queue::DelayQueue;
pub use self::delay::Delay;
pub use self::error::Error;
pub use self::interval::Interval;
pub use self::timer::{with_default, Timer};

use std::time::{Duration, Instant};

/// Create a Future that completes in `duration` from now.
pub fn sleep(duration: Duration) -> Delay {
    Delay::new(Instant::now() + duration)
}

// ===== Internal utils =====

enum Round {
    Up,
    Down,
}

/// Convert a `Duration` to milliseconds, rounding up and saturating at
/// `u64::MAX`.
///
/// The saturating is fine because `u64::MAX` milliseconds are still many
/// million years.
#[inline]
fn ms(duration: Duration, round: Round) -> u64 {
    const NANOS_PER_MILLI: u32 = 1_000_000;
    const MILLIS_PER_SEC: u64 = 1_000;

    // Round up.
    let millis = match round {
        Round::Up => (duration.subsec_nanos() + NANOS_PER_MILLI - 1) / NANOS_PER_MILLI,
        Round::Down => duration.subsec_nanos() / NANOS_PER_MILLI,
    };

    duration.as_secs().saturating_mul(MILLIS_PER_SEC).saturating_add(millis as u64)
}
