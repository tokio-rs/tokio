//! Synchronization primitives

mod cancellation_token;
pub use cancellation_token::{Aborted, CancellationToken, WaitForCancellationFuture, Wrapped};

mod intrusive_double_linked_list;
