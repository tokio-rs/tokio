//! The scope module provides the `scope` method, which enables structured concurrency

mod cancellation_token;
mod intrusive_double_linked_list;
mod wait_group;

mod scope;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use scope::{
    scope, scope_with_options, ScopeCancelBehavior, ScopeDropBehavior, ScopeHandle, ScopeOptions,
    ScopedJoinHandle,
};
