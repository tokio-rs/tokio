cfg_io_driver! {
    pub(crate) mod bit;
    pub(crate) mod slab;
}

#[cfg(any(feature = "rt-threaded", feature = "macros"))]
mod rand;

cfg_rt_threaded! {
    mod pad;
    pub(crate) use pad::CachePadded;

    pub(crate) use rand::FastRand;

    mod try_lock;
    pub(crate) use try_lock::TryLock;
}

cfg_macros! {
    pub use rand::thread_rng_n;
}
