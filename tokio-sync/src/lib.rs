#![doc(html_root_url = "https://docs.rs/tokio-sync/0.1.5")]
#![deny(
    missing_debug_implementations,
    missing_docs,
    unreachable_pub,
    rust_2018_idioms
)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]
#![feature(async_await)]

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

pub mod lock;
mod loom;
pub mod mpsc;
pub mod oneshot;
pub mod semaphore;
pub mod task;
pub mod watch;
