cfg_io_driver! {
    pub(crate) mod bit;
    pub(crate) mod slab;
}

#[cfg(any(feature = "sync", feature = "rt-core"))]
pub(crate) mod linked_list;

#[cfg(any(feature = "rt-threaded", feature = "macros", feature = "stream"))]
mod rand;

mod wake;
pub(crate) use wake::{waker_ref, Wake};

cfg_rt_threaded! {
    pub(crate) use rand::FastRand;

    mod try_lock;
    pub(crate) use try_lock::TryLock;
}

#[cfg(any(feature = "macros", feature = "stream"))]
#[cfg_attr(not(feature = "macros"), allow(unreachable_pub))]
pub use rand::thread_rng_n;
