#![cfg_attr(loom, allow(dead_code, unreachable_pub, unused_imports))]

//! Future-aware synchronization
//!
//! This module is enabled with the **`sync`** feature flag.
//!
//! Tasks sometimes need to communicate with each other. This module contains
//! basic abstractions for doing so:
//!
//! - [oneshot](oneshot/index.html), a way of sending a single value
//!   from one task to another.
//! - [mpsc](mpsc/index.html), a multi-producer, single-consumer channel for
//!   sending values between tasks.
//! - [`Mutex`](struct.Mutex.html), an asynchronous `Mutex`-like type.
//! - [watch](watch/index.html), a single-producer, multi-consumer channel that
//!   only stores the **most recently** sent value.

cfg_sync! {
    mod barrier;
    pub use barrier::{Barrier, BarrierWaitResult};

    pub mod mpsc;

    mod mutex;
    pub use mutex::{Mutex, MutexGuard};

    pub mod oneshot;

    pub(crate) mod semaphore_ll;
    mod semaphore;
    pub use semaphore::{Semaphore, SemaphorePermit};

    mod task;
    pub(crate) use task::AtomicWaker;

    pub mod watch;
}

cfg_not_sync! {
    cfg_atomic_waker_impl! {
        mod task;
        pub(crate) use task::AtomicWaker;
    }

    #[cfg(any(
            feature = "rt-core",
            feature = "process",
            feature = "signal"))]
    pub(crate) mod oneshot;

    cfg_signal! {
        pub(crate) mod mpsc;
        pub(crate) mod semaphore_ll;
    }
}

/// Unit tests
#[cfg(test)]
mod tests;
