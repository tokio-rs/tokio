#[cfg(any(
    feature = "fs",
    feature = "net",
    feature = "process",
    feature = "sync",
    feature = "signal",
    feature = "time",
))]
pub(crate) mod linked_list;

#[cfg(any(feature = "macros"))]
mod rand;

#[cfg(any(feature = "macros"))]
#[cfg_attr(not(feature = "macros"), allow(unreachable_pub))]
pub use rand::thread_rng_n;
