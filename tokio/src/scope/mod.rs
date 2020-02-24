#![cfg_attr(loom, allow(dead_code, unreachable_pub, unused_imports))]

//! Tools for structuring concurrent tasks
//!
//! Tokio tasks can run completely independent of each other. However it is
//! often useful to group tasks which try to fulfill a common goal.
//! These groups of tasks should share the same lifetime. If the task group is
//! no longer needed all tasks should stop. If one task errors, the other tasks
//! might no longer be needed and should also be cancelled.
//!
//! The utilities inside this module allow to group tasks by following the
//! concept of structured concurrency.

mod cancellation_token;
pub use cancellation_token::{CancellationToken, WaitForCancellationFuture};

/// Unit tests
#[cfg(test)]
mod tests;
