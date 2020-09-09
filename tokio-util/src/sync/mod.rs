//! Synchronization primitives

mod cancellation_token;
pub use cancellation_token::{
    wrapped::{Aborted, Wrapped},
    CancellationToken, WaitForCancellationFuture,
};

mod intrusive_double_linked_list;
