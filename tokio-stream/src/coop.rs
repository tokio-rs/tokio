//! Cooperative budgeting with a no-op implementation when `rt` is disabled.

#[cfg(feature = "rt")]
pub(crate) use tokio::task::coop::poll_proceed;

#[cfg(not(feature = "rt"))]
pub(crate) use without_rt::poll_proceed;

#[cfg(not(feature = "rt"))]
mod without_rt {
    use std::task::{Context, Poll};

    pub(crate) struct RestoreOnPending;

    impl RestoreOnPending {
        pub(crate) fn made_progress(&self) {}
    }

    pub(crate) fn poll_proceed(_: &mut Context<'_>) -> Poll<RestoreOnPending> {
        Poll::Ready(RestoreOnPending)
    }
}
