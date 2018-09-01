//! Utilities for working with Tokio.
//!
//! This module contains utilities that are useful for working with Tokio.
//! Currently, this only includes [`FutureExt`], but this may grow over time.
//!
//! [`FutureExt`]: trait.FutureExt.html

mod future;

pub use self::future::FutureExt;
