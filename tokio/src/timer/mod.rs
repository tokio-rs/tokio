//! Utilities for tracking time.
//!
//! This module provides a number of types for executing code after a set period
//! of time.
//!
//! * [`Delay`][Delay] is a future that does no work and completes at a specific `Instant`
//!   in time.
//!
//! * [`Interval`][Interval] is a stream yielding a value at a fixed period. It
//!   is initialized with a `Duration` and repeatedly yields each time the
//!   duration elapses.
//!
//! * [`Timeout`][Timeout]: Wraps a future or stream, setting an upper bound to the
//!   amount of time it is allowed to execute. If the future or stream does not
//!   complete in time, then it is canceled and an error is returned.
//!
//! * [`DelayQueue`]: A queue where items are returned once the requested delay
//!   has expired.
//!
//! These types are sufficient for handling a large number of scenarios
//! involving time.
//!
//! These types must be used from within the context of the
//! [`Runtime`][runtime] or a timer context must be setup explicitly. See the
//! [`tokio-timer`][tokio-timer] crate for more details on how to setup a timer
//! context.
//!
//! # Examples
//!
//! Wait 100ms and print "Hello World!"
//!
//! ```
//! use tokio::timer::delay_for;
//!
//! use std::time::Duration;
//!
//!
//! #[tokio::main]
//! async fn main() {
//!     delay_for(Duration::from_millis(100)).await;
//!     println!("100 ms have elapsed");
//! }
//! ```
//!
//! Require that an operation takes no more than 300ms. Note that this uses the
//! [`timeout`][ext] function on the [`FutureExt`][ext] trait. This trait is
//! included in the prelude.
//!
//! ```
//! use tokio::prelude::*;
//! use std::time::Duration;
//!
//! async fn long_future() {
//!     // do work here
//! }
//!
//! # async fn dox() {
//! let res = long_future()
//!     .timeout(Duration::from_secs(1))
//!     .await;
//!
//! if res.is_err() {
//!     println!("operation timed out");
//! }
//! # }
//! ```
//!
//! [runtime]: ../runtime/struct.Runtime.html
//! [tokio-timer]: https://docs.rs/tokio-timer
//! [ext]: ../util/trait.FutureExt.html#method.timeout
//! [Timeout]: struct.Timeout.html
//! [Delay]: struct.Delay.html
//! [Interval]: struct.Interval.html
//! [`DelayQueue`]: struct.DelayQueue.html

pub mod clock;

pub mod delay_queue;
#[doc(inline)]
pub use self::delay_queue::DelayQueue;

pub mod throttle;

// TODO: clean this up
#[allow(clippy::module_inception)]
pub mod timer;
pub use timer::{set_default, Timer};

pub mod timeout;
#[doc(inline)]
pub use timeout::Timeout;

mod atomic;

mod delay;
pub use self::delay::Delay;

mod error;
pub use error::Error;

mod interval;
pub use interval::Interval;

mod wheel;

use std::time::{Duration, Instant};

/// Create a Future that completes at `deadline`.
pub fn delay(deadline: Instant) -> Delay {
    Delay::new(deadline)
}

/// Create a Future that completes in `duration` from now.
///
/// Equivalent to `delay(tokio::timer::clock::now() + duration)`. Analogous to `std::thread::sleep`.
pub fn delay_for(duration: Duration) -> Delay {
    delay(clock::now() + duration)
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
        Round::Down => duration.subsec_millis(),
    };

    duration
        .as_secs()
        .saturating_mul(MILLIS_PER_SEC)
        .saturating_add(u64::from(millis))
}
