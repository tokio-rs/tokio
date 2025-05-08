cfg_macros! {
    pub use crate::future::maybe_done::maybe_done;

    pub use std::future::poll_fn;

    #[doc(hidden)]
    pub fn thread_rng_n(n: u32) -> u32 {
        crate::runtime::context::thread_rng_n(n)
    }

    cfg_coop! {
        #[doc(hidden)]
        #[inline]
        pub fn poll_budget_available(cx: &mut Context<'_>) -> Poll<()> {
            crate::task::coop::poll_budget_available(cx)
        }
    }

    cfg_not_coop! {
        #[doc(hidden)]
        #[inline]
        pub fn poll_budget_available(_: &mut Context<'_>) -> Poll<()> {
            Poll::Ready(())
        }
    }
}

pub use std::future::{Future, IntoFuture};
pub use std::pin::Pin;
pub use std::task::{Context, Poll};

#[doc(hidden)]
#[derive(Default, Debug)]
pub struct Rotator<const COUNT: u32> {
    next: u32,
}

impl<const COUNT: u32> Rotator<COUNT> {
    #[doc(hidden)]
    #[inline]
    pub fn num_skip(&mut self) -> u32 {
        let num_skip = self.next;
        self.next += 1;
        if self.next == COUNT {
            self.next = 0;
        }
        num_skip
    }
}

#[doc(hidden)]
#[derive(Default, Debug)]
pub struct BiasedRotator {}

impl BiasedRotator {
    #[doc(hidden)]
    #[inline]
    pub fn num_skip(&mut self) -> u32 {
        0
    }
}
