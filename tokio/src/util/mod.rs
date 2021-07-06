cfg_io_driver! {
    pub(crate) mod bit;
    pub(crate) mod slab;
}

#[cfg(any(
    feature = "fs",
    feature = "net",
    feature = "process",
    feature = "rt",
    feature = "sync",
    feature = "signal",
    feature = "time",
))]
pub(crate) mod linked_list;

#[cfg(any(feature = "rt-multi-thread", feature = "macros"))]
mod rand;

cfg_rt! {
    mod wake;
    pub(crate) use wake::WakerRef;
    pub(crate) use wake::{waker_ref, Wake};
}

cfg_rt_multi_thread! {
    pub(crate) use self::rand::FastRand;

    mod try_lock;
    pub(crate) use try_lock::TryLock;
}

pub(crate) mod trace;

#[cfg(any(feature = "macros"))]
#[cfg_attr(not(feature = "macros"), allow(unreachable_pub))]
pub use self::rand::thread_rng_n;

#[cfg(any(
    feature = "rt",
    feature = "time",
    feature = "net",
    feature = "process",
    all(unix, feature = "signal")
))]
pub(crate) mod error;
