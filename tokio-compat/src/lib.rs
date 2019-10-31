//! Compatibility between `tokio` 0.2 and legacy versions.
//!
//! ## Overview
//!
//! This crate provides compatibility runtimes that allow running both `futures` 0.1
//! futures that use `tokio` 0.1 runtime services _and_ `std::future` futures that
//! use `tokio` 0.2 runtime services.
//!
//! ### Examples
//!
//! Spawning both `tokio` 0.1 and `tokio` 0.2 futures:
//!
//! ```rust
//! use futures_01::future::lazy;
//!
//! tokio_compat::run(lazy(|| {
//!     // spawn a `futures` 0.1 future using the `spawn` function from the
//!     // `tokio` 0.1 crate:
//!     tokio_01::spawn(lazy(|| {
//!         println!("hello from tokio 0.1!");
//!         Ok(())
//!     }));
//!
//!     // spawn an `async` block future on the same runtime using `tokio`
//!     // 0.2's `spawn`:
//!     tokio_02::spawn(async {
//!         println!("hello from tokio 0.2!");
//!     });
//!
//!     Ok(())
//! }))
//! ```
//!
//! Futures on the compat runtime can use `timer` APIs from both 0.1 and 0.2
//! versions of `tokio`:
//!
//! ```rust
//! use std::time::{Duration, Instant};
//! use tokio_compat::prelude::*;
//!
//! tokio_compat::run_std(async {
//!     // Wait for a `tokio` 0.1 `Delay`...
//!     let when = Instant::now() + Duration::from_millis(10);
//!     tokio_01::timer::Delay::new(when)
//!         // convert the delay future into a `std::future` that we can `await`.
//!         .compat()
//!         .await
//!         .expect("tokio 0.1 timer should work!");
//!     println!("10 ms have elapsed");
//!
//!     // Wait for a `tokio` 0.2 `Delay`...
//!     let when = Instant::now() + Duration::from_millis(20);
//!     tokio_02::timer::delay(when).await;
//!     println!("20 ms have elapsed");
//! });
//! ```
//!
//! ## Feature Flags
//!
//! - `rt-current-thread`: enables the `current_thread` compatibilty runtime
//! - `rt-full`: enables the `current_thread` and threadpool compatibility
//!   runtimes (enabled by default)
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
#[cfg(any(feature = "rt-current-thread", feature = "rt-full"))]
pub mod runtime;
#[cfg(feature = "rt-full")]
pub use self::runtime::{run, run_std};
pub mod prelude;
