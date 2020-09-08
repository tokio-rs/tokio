//! Synchronization primitives

mod cancellation_token;
pub use cancellation_token::{CancellationToken, WaitForCancellationFuture, Aborted, Wrapped};

mod intrusive_double_linked_list;
