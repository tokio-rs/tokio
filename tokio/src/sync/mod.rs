//! Future-aware synchronization
//!
//! This module is enabled with the **`sync`** feature flag.
//!
//! Tasks sometimes need to communicate with each other. This module contains
//! two basic abstractions for doing so:
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

    pub(crate) mod semaphore;

    mod task;
    pub(crate) use task::AtomicWaker;

    pub mod watch;
}

cfg_not_sync! {
    cfg_atomic_waker! {
        mod task;
        pub(crate) use task::AtomicWaker;
    }

    cfg_rt_threaded! {
        pub(crate) mod oneshot;
    }

    cfg_signal! {
        pub(crate) mod mpsc;
        pub(crate) mod semaphore;

        cfg_not_rt_threaded! {
            pub(crate) mod oneshot;
        }
    }
}

/// Unit tests
#[cfg(test)]
mod tests;
