#![doc(html_root_url = "https://docs.rs/tokio-timer/0.2.10")]
#![deny(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

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

macro_rules! ready {
    ($e:expr) => {
        match $e {
            ::std::task::Poll::Ready(v) => v,
            ::std::task::Poll::Pending => return ::std::task::Poll::Pending,
        }
    };
}

pub mod clock;
#[cfg(feature = "delay-queue")]
pub mod delay_queue;
#[cfg(feature = "throttle")]
pub mod throttle;
pub mod timeout;
pub mod timer;

mod atomic;
mod delay;
mod error;
#[cfg(feature = "interval")]
mod interval;
mod wheel;

pub use delay::Delay;
#[cfg(feature = "delay-queue")]
#[doc(inline)]
pub use delay_queue::DelayQueue;
pub use error::Error;
#[cfg(feature = "interval")]
pub use interval::Interval;
#[doc(inline)]
pub use timeout::Timeout;
pub use timer::{with_default, Timer};

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

    duration
        .as_secs()
        .saturating_mul(MILLIS_PER_SEC)
        .saturating_add(millis as u64)
}
