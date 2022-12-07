//! Implementation of an atomic u64 cell. On 64 bit platforms, this is a
//! re-export of `AtomicU64`. On 32 bit platforms, this is implemented using a
//! `Mutex`.

// `AtomicU64` can only be used on targets with `target_has_atomic` is 64 or greater.
// Once `cfg_target_has_atomic` feature is stable, we can replace it with
// `#[cfg(target_has_atomic = "64")]`.
// Refs: https://github.com/rust-lang/rust/tree/master/src/librustc_target
cfg_has_atomic_u64! {
    pub(crate) use std::sync::atomic::AtomicU64;
}

cfg_not_has_atomic_u64! {
    #[path = "atomic_u64_as_mutex.rs"]
    mod atomic_u64_as_mutex;

    pub(crate) use atomic_u64_as_mutex::AtomicU64;
}
