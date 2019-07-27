//! Utilities for working with Tokio.
//!
//! This module contains utilities that are useful for working with Tokio.
//! Currently, this only includes [`FutureExt`] and [`StreamExt`], but this
//! may grow over time.
//!
//! [`FutureExt`]: trait.FutureExt.html
//! [`StreamExt`]: trait.StreamExt.html

mod future;
mod stream;

pub use self::future::FutureExt;
pub use self::stream::StreamExt;
