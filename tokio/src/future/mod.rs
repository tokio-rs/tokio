#![cfg_attr(not(feature = "macros"), allow(unreachable_pub))]

//! Asynchronous values.

#[cfg(any(feature = "macros", feature = "process"))]
pub(crate) mod maybe_done;

mod poll_fn;
pub use poll_fn::poll_fn;

cfg_not_loom! {
    mod ready;
    pub(crate) use ready::{ok, Ready};
}

cfg_process! {
    mod try_join;
    pub(crate) use try_join::try_join3;
}

cfg_sync! {
    mod block_on;
    pub(crate) use block_on::block_on;
}
