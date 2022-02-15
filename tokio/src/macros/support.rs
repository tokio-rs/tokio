cfg_macros! {
    pub use crate::future::poll_fn;
    pub use crate::future::maybe_done::maybe_done;
    pub use crate::util::thread_rng_n;
}

// Re-export not needed since it's pulling from std
#[deprecated(note = "Prefer importing std::future::Future directly")]
pub use std::future::Future;
#[deprecated(note = "Prefer importing std::pin::Pin directly")]
pub use std::pin::Pin;
#[deprecated(note = "Prefer importing std::task::Poll directly")]
pub use std::task::Poll;
