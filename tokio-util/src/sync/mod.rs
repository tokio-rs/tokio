//! Synchronization primitives

mod cancellation_token;
pub use cancellation_token::{CancellationToken, WaitForCancellationFuture};

mod intrusive_double_linked_list;
