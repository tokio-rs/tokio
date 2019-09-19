#![doc(html_root_url = "https://docs.rs/tokio-sync/0.2.0-alpha.5")]
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

//! Asynchronous synchronization primitives.
//!
//! This crate provides primitives for synchronizing asynchronous tasks.

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
