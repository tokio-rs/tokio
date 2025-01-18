cfg_macros! {
    pub use crate::future::maybe_done::maybe_done;

    pub use std::future::poll_fn;

    #[doc(hidden)]
    pub fn thread_rng_n(n: u32) -> u32 {
        crate::runtime::context::thread_rng_n(n)
    }

    pub fn poll_budget_available(cx: &mut std::task::Context<'_>) -> std::task::Poll<()> {
        #[cfg(feature = "rt")]
        { crate::task::poll_budget_available(cx) }
        #[cfg(not(feature = "rt"))]
        {
            // Use the `cx` argument to suppress unused variable warnings
            let _ = cx;
            std::task::Poll::Ready(())
        }
    }
}

pub use std::future::{Future, IntoFuture};
pub use std::pin::Pin;
pub use std::task::Poll;
