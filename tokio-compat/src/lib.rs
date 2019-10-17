//! Compatibility between `tokio` 0.2 and legacy versions.
#[cfg(any(feature = "rt-current-thread", feature = "rt-full"))]
pub mod runtime;
#[cfg(feature = "rt-full")]
pub use self::runtime::{run, run_std};
pub mod prelude;
