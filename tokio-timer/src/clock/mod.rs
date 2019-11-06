//! A configurable source of time.
//!
//! This module provides an API to get the current instant in such a way that
//! the source of time may be configured. This allows mocking out the source of
//! time in tests.
//!
//! The [`now`][n] function returns the current [`Instant`]. By default, it delegates
//! to [`Instant::now`].
//!
//! The source of time used by [`now`][n] can be configured by implementing the
//! [`Now`] trait and passing an instance to [`with_default`].
//!
//! [n]: fn.now.html
//! [`Now`]: trait.Now.html
//! [`Instant`]: https://doc.rust-lang.org/std/time/struct.Instant.html
//! [`Instant::now`]: https://doc.rust-lang.org/std/time/struct.Instant.html#method.now
//! [`with_default`]: fn.with_default.html

mod clock;
mod now;

pub use self::clock::{now, set_default, with_default, Clock, DefaultGuard};
pub use self::now::Now;
