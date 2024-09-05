cfg_macros! {
    pub use crate::future::maybe_done::maybe_done;

    pub use std::future::poll_fn;

    #[doc(hidden)]
    pub fn thread_rng_n(n: u32) -> u32 {
        crate::runtime::context::thread_rng_n(n)
    }
}

pub use std::future::{Future, IntoFuture};
pub use std::pin::Pin;
pub use std::task::Poll;
