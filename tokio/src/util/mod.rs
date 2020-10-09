cfg_io_driver! {
    pub(crate) mod bit;
    pub(crate) mod slab;
}

#[cfg(any(
    feature = "fs",
    feature = "process",
    feature = "rt-core",
    feature = "rt-util",
    feature = "sync",
    feature = "signal",
    feature = "tcp",
    feature = "udp",
    feature = "uds",
))]
pub(crate) mod linked_list;

#[cfg(any(feature = "rt-threaded", feature = "macros", feature = "stream"))]
mod rand;

cfg_rt_core! {
    mod wake;
    pub(crate) use wake::WakerRef;
    pub(crate) use wake::{waker_ref, Wake};
}

cfg_rt_threaded! {
    pub(crate) use rand::FastRand;

    mod try_lock;
    pub(crate) use try_lock::TryLock;
}

pub(crate) mod trace;

#[cfg(any(feature = "macros", feature = "stream"))]
#[cfg_attr(not(feature = "macros"), allow(unreachable_pub))]
pub use rand::thread_rng_n;
