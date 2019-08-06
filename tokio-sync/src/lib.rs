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

macro_rules! pin_mut {
    ($($x:ident),*) => { $(
        // Move the value to ensure that it is owned
        let mut $x = $x;
        // Shadow the original binding so that it can't be directly accessed
        // ever again.
        #[allow(unused_mut)]
        let mut $x = unsafe {
            std::pin::Pin::new_unchecked(&mut $x)
        };
    )* }
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
