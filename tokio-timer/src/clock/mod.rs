//! A configurable source of time.
//!
//! This module provides an API to get the current instant in such a way that
//! the source of time may be configured. This allows mocking out the source of
//! time in tests.
//!
//! The [`now`][n] function returns the current `Instant`. By default, it delegates
//! to [`Instant::now`][std].
//!
//! The source of time used by [`now`] can be configured by implementing the
//! [`Now`] trait and passing an instance to [`with_default`].
//!
//! [n]: fn.now.html
//! [`Now`]: trait.Now.html
//! [std]: https://doc.rust-lang.org/std/time/struct.Instant.html
//! [`with_default`]: fn.with_default.html

mod clock;
mod now;

pub use self::clock::{Clock, now, with_default};
pub use self::now::Now;
