//! Utilities for working with Tokio.
//!
//! This module contains utilities that are useful for working with Tokio.
//! Currently, this only includes [`FutureExt`][FutureExt]. However, this will
//! include over time.

mod future;

pub use self::future::FutureExt;
