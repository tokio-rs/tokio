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

macro_rules! debug {
    ($($t:tt)*) => {
        if false {
            println!($($t)*);
        }
    }
}

macro_rules! if_loom {
    ($($t:tt)*) => {{
        #[cfg(loom)]
        const LOOM: bool = true;
        #[cfg(not(loom))]
        const LOOM: bool = false;

        if LOOM {
            $($t)*
        }
    }}
}

mod barrier;
pub use barrier::{Barrier, BarrierWaitResult};

pub mod mpsc;

mod mutex;
pub use mutex::{Mutex, MutexGuard};

pub mod oneshot;

pub mod semaphore;

mod task;
pub(crate) use task::AtomicWaker;

pub mod watch;

/// Unit tests
#[cfg(test)]
mod tests;
