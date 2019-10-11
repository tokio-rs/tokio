#![doc(html_root_url = "https://docs.rs/tokio-sync/0.2.0-alpha.6")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(intra_doc_link_resolution_failure)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

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

macro_rules! if_fuzz {
    ($($t:tt)*) => {{
        if false { $($t)* }
    }}
}

mod barrier;
mod loom;
pub mod mpsc;
mod mutex;
pub mod oneshot;
pub mod semaphore;
mod task;
pub mod watch;

pub use barrier::{Barrier, BarrierWaitResult};
pub use mutex::{Mutex, MutexGuard};
pub use task::AtomicWaker;
