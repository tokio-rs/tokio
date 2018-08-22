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
//! * [`Deadline`][Deadline] wraps a future, requiring that it completes before
//!   a specified `Instant` in time. If the future does not complete in time,
//!   then it is canceled and an error is returned.
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
//! use tokio::prelude::*;
//! use tokio::timer::Delay;
//!
//! use std::time::{Duration, Instant};
//!
//! let when = Instant::now() + Duration::from_millis(100);
//!
//! tokio::run({
//!     Delay::new(when)
//!         .map_err(|e| panic!("timer failed; err={:?}", e))
//!         .and_then(|_| {
//!             println!("Hello world!");
//!             Ok(())
//!         })
//! })
//! ```
//!
//! Require that an operation takes no more than 300ms. Note that this uses the
//! [`deadline`][ext] function on the [`FutureExt`][ext] trait. This trait is
//! included in the prelude.
//!
//! ```
//! # extern crate futures;
//! # extern crate tokio;
//! use tokio::prelude::*;
//!
//! use std::time::{Duration, Instant};
//!
//! fn long_op() -> Box<Future<Item = (), Error = ()> + Send> {
//!     // ...
//! # Box::new(futures::future::ok(()))
//! }
//!
//! # fn main() {
//! let when = Instant::now()  + Duration::from_millis(300);
//!
//! tokio::run({
//!     long_op()
//!         .deadline(when)
//!         .map_err(|e| {
//!             println!("operation timed out");
//!         })
//! })
//! # }
//! ```
//!
//! [runtime]: ../runtime/struct.Runtime.html
//! [tokio-timer]: https://docs.rs/tokio-timer
//! [ext]: ../util/trait.FutureExt.html#method.deadline
//! [Deadline]: struct.Deadline.html
//! [Delay]: struct.Delay.html
//! [Interval]: struct.Interval.html
//! [`DelayQueue`]: struct.DelayQueue.html

pub use tokio_timer::{
    delay_queue,
    Deadline,
    DeadlineError,
    DelayQueue,
    Error,
    Interval,
    Delay,
};
