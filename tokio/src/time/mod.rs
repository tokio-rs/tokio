//! Utilities for tracking time.
//!
//! This module provides a number of types for executing code after a set period
//! of time.
//!
//! * `Sleep` is a future that does no work and completes at a specific `Instant`
//!   in time.
//!
//! * `Interval` is a stream yielding a value at a fixed period. It is
//!   initialized with a `Duration` and repeatedly yields each time the duration
//!   elapses.
//!
//! * `Timeout`: Wraps a future or stream, setting an upper bound to the amount
//!   of time it is allowed to execute. If the future or stream does not
//!   complete in time, then it is canceled and an error is returned.
//!
//! These types are sufficient for handling a large number of scenarios
//! involving time.
//!
//! These types must be used from within the context of the `Runtime`.
//!
//! # Examples
//!
//! Wait 100ms and print "100 ms have elapsed"
//!
//! ```
//! use tokio::time::sleep;
//!
//! use std::time::Duration;
//!
//!
//! #[tokio::main]
//! async fn main() {
//!     sleep(Duration::from_millis(100)).await;
//!     println!("100 ms have elapsed");
//! }
//! ```
//!
//! Require that an operation takes no more than 300ms. Note that this uses the
//! `timeout` function on the `FutureExt` trait. This trait is included in the
//! prelude.
//!
//! ```
//! use tokio::time::{timeout, Duration};
//!
//! async fn long_future() {
//!     // do work here
//! }
//!
//! # async fn dox() {
//! let res = timeout(Duration::from_secs(1), long_future()).await;
//!
//! if res.is_err() {
//!     println!("operation timed out");
//! }
//! # }
//! ```
//!
//! A simple example using [`interval`] to execute a task every two seconds.
//!
//! The difference between [`interval`] and [`sleep`] is that an
//! [`interval`] measures the time since the last tick, which means that
//! `.tick().await` may wait for a shorter time than the duration specified
//! for the interval if some time has passed between calls to `.tick().await`.
//!
//! If the tick in the example below was replaced with [`sleep`], the task
//! would only be executed once every three seconds, and not every two
//! seconds.
//!
//! ```
//! use tokio::time;
//!
//! async fn task_that_takes_a_second() {
//!     println!("hello");
//!     time::sleep(time::Duration::from_secs(1)).await
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let interval = time::interval(time::Duration::from_secs(2));
//!     tokio::pin!(interval);
//!
//!     for _i in 0..5 {
//!         interval.as_mut().tick().await;
//!         task_that_takes_a_second().await;
//!     }
//! }
//! ```
//!
//! [`sleep`]: crate::time::sleep()
//! [`interval`]: crate::time::interval()

#[cfg(feature = "test-util")]
pub use t10::time::{advance, pause, resume};

mod sleep;
#[doc(inline)]
pub use sleep::{sleep, sleep_until, Sleep};

pub use t10::time::error;

mod instant;
pub use self::instant::Instant;

pub use t10::time::{interval, interval_at, Interval};

#[doc(inline)]
pub use t10::time::{timeout, timeout_at, Timeout};

#[cfg(test)]
#[cfg(not(loom))]
mod tests;

// Re-export for convenience
#[doc(no_inline)]
pub use std::time::Duration;
