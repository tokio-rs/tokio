cfg_macros! {
    pub use crate::future::{maybe_done, poll_fn};
    pub use crate::util::thread_rng_n;
}

pub use std::future::Future;
pub use std::pin::Pin;
pub use std::task::Poll;
