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

#[cfg(any(feature = "rt-multi-thread", feature = "macros", feature = "stream"))]
mod rand;

cfg_rt! {
    mod wake;
}

pub(crate) mod trace;

#[cfg(any(feature = "macros", feature = "stream"))]
#[cfg_attr(not(feature = "macros"), allow(unreachable_pub))]
pub use rand::thread_rng_n;
