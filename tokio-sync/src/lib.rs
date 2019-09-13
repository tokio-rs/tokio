#![doc(html_root_url = "https://docs.rs/tokio-sync/0.2.0-alpha.4")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(intra_doc_link_resolution_failure)]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

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

mod lock;
mod loom;
pub mod mpsc;
pub mod oneshot;
pub mod semaphore;
mod task;
pub mod watch;

pub use lock::{Lock, LockGuard};
pub use task::AtomicWaker;
