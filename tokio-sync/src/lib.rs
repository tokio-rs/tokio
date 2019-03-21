#![doc(html_root_url = "https://docs.rs/tokio-sync/0.1.4")]
#![deny(missing_debug_implementations, missing_docs, unreachable_pub)]
#![cfg_attr(test, deny(warnings))]

//! Asynchronous synchronization primitives.
//!
//! This crate provides primitives for synchronizing asynchronous tasks.

extern crate fnv;
#[macro_use]
extern crate futures;

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
