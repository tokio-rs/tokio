//! A configurable source of time.
//!
//! This module provides the [`now`][n] function, which returns an `Instant`
//! representing "now". The source of time used by this function is configurable
//! (via the [`tokio-timer`] crate) and allows mocking out the source of time in
//! tests or performing caching operations to reduce the number of syscalls.
//!
//! Note that, because the source of time is configurable, it is possible to
//! observe non-monotonic behavior when calling [`now`][n] from different
//! executors.
//!
//! [n]: fn.now.html
//! [`tokio-timer`]: https://docs.rs/tokio-timer/0.2/tokio_timer/clock/index.html

pub use tokio_timer::clock::now;
