//! A configurable source of time.
//!
//! This module provides the [`now`][n] function, which returns an `Instant`
//! representing "now". The source of time used by this function is configurable
//! and allows mocking out the source of time in tests or performing caching
//! operations to reduce the number of syscalls.
//!
//! Note that, because the source of time is configurable, it is possible to
//! observe non-monotonic behavior when calling [`now`][n] from different
//! executors.
//!
//! [n]: fn.now.html

pub use crate::timer::clock::now;
