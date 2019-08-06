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
//! #![feature(async_await)]
//!
//! use tokio::prelude::*;
//! use tokio::timer::Delay;
//!
//! use std::time::{Duration, Instant};
//!
//!
//! #[tokio::main]
//! async fn main() {
//!     let when = tokio::clock::now() + Duration::from_millis(100);
//!     Delay::new(when).await;
//!     println!("100 ms have elapsed");
//! }
//! ```
//!
//! Require that an operation takes no more than 300ms. Note that this uses the
//! [`timeout`][ext] function on the [`FutureExt`][ext] trait. This trait is
//! included in the prelude.
//!
//! ```
//! #![feature(async_await)]
//!
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

pub use tokio_timer::{delay_queue, timeout, Delay, DelayQueue, Error, Interval, Timeout};
