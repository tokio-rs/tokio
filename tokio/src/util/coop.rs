//! Cooperative budgeting for utilities that also compile without coop support.

cfg_coop! {
    pub(crate) use crate::task::coop::poll_proceed;
}

cfg_not_coop! {
    use std::task::{Context, Poll};

    pub(crate) struct RestoreOnPending;

    impl RestoreOnPending {
        pub(crate) fn made_progress(&self) {}
    }

    pub(crate) fn poll_proceed(_: &mut Context<'_>) -> Poll<RestoreOnPending> {
        Poll::Ready(RestoreOnPending)
    }
}
