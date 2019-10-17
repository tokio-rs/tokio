//! Compatibility between `tokio` 0.2 and legacy versions.
#[cfg(feature = "runtime")]
pub mod runtime;
#[cfg(feature = "runtime")]
pub use self::runtime::{run, run_std};
pub mod prelude;
