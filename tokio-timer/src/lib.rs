#![doc(html_root_url = "https://docs.rs/tokio-timer/0.2.10")]
#![deny(missing_docs, warnings, missing_debug_implementations)]

//! Utilities for tracking time.
//!
//! This crate provides a number of utilities for working with periods of time:
//!
//! * [`Delay`]: A future that completes at a specified instant in time.
//!
//! * [`Interval`] A stream that yields at fixed time intervals.
//!
//! * [`Throttle`]: Throttle down a stream by enforcing a fixed delay between items.
//!
//! * [`Timeout`]: Wraps a future or stream, setting an upper bound to the
//!   amount of time it is allowed to execute. If the future or stream does not
//!   complete in time, then it is canceled and an error is returned.
//!
//! * [`DelayQueue`]: A queue where items are returned once the requested delay
//!   has expired.
//!
//! These three types are backed by a [`Timer`] instance. In order for
//! [`Delay`], [`Interval`], and [`Timeout`] to function, the associated
//! [`Timer`] instance must be running on some thread.
//!
//! [`Delay`]: struct.Delay.html
//! [`DelayQueue`]: struct.DelayQueue.html
//! [`Throttle`]: throttle/struct.Throttle.html
//! [`Timeout`]: struct.Timeout.html
//! [`Interval`]: struct.Interval.html
//! [`Timer`]: timer/struct.Timer.html

extern crate tokio_executor;

extern crate crossbeam_utils;
#[macro_use]
extern crate futures;
extern crate slab;

pub mod clock;
pub mod delay_queue;
pub mod throttle;
pub mod timeout;
pub mod timer;

mod atomic;
mod deadline;
mod delay;
mod error;
mod interval;
mod wheel;

#[deprecated(since = "0.2.6", note = "use Timeout instead")]
#[doc(hidden)]
#[allow(deprecated)]
pub use self::deadline::{Deadline, DeadlineError};
pub use self::delay::Delay;
#[doc(inline)]
pub use self::delay_queue::DelayQueue;
pub use self::error::Error;
pub use self::interval::Interval;
#[doc(inline)]
pub use self::timeout::Timeout;
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

/// Timer precision
#[derive(Debug, Copy, Clone)]
pub struct Resolution {
    /// nanoseconds per unit of time
    pub nanos_per_unit: u64,
    /// units of time in a second
    pub units_per_sec: u64,
}

impl Resolution {
    /// Convert a `Duration` to base units, rounding up and saturating at
    /// `u64::MAX`.
    fn to_base_units(&self, duration: Duration, round: Round) -> u64 {
        // Round up.
        let millis = match round {
            Round::Up => {
                (duration.subsec_nanos() as u64 + self.nanos_per_unit - 1) / self.nanos_per_unit
            }
            Round::Down => duration.subsec_nanos() as u64 / self.nanos_per_unit,
        };

        duration
            .as_secs()
            .saturating_mul(self.units_per_sec)
            .saturating_add(millis as u64)
    }

    /// Convert from base units back to `Duration`
    fn from_base_units(&self, units: u64) -> Duration {
        Duration::from_nanos(units.saturating_mul(self.nanos_per_unit))
    }
}
