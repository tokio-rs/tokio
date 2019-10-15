//! Runtimes compatible with both `tokio` 0.1 and `tokio` 0.2 futures.
//!
//! This module is similar to the [`tokio::runtime`] module. However, the
//! runtimes in this crate are capable of executing both `futures` 0.1 futures
//! that use the `tokio` 0.1 runtime services (i.e. `timer`, `reactor`, and
//! `executor`), **and** `std::future` futures that use the `tokio` 0.2 runtime
//! services.
//!
//! The `futures` crate's [`compat` module][futures-compat] provides
//! interoperability between `futures` 0.1 and `std::future` _future types_
//! (e.g. implementing `std::future::Future` for a type that implements the
//! `futures` 0.1 `Future` trait). However, this on its own is insufficient to
//! run code written against `tokio` 0.1 on a `tokio` 0.2 runtime, if that code
//! also relies on `tokio`'s runtime services. If legacy tasks are executed that
//! rely on `tokio::timer`, perform IO using `tokio`'s reactor, or call
//! `tokio::spawn`, those API calls will fail unless there is also a runtime
//! compatibility layer.
//!
//! `tokio-compat`'s `runtime` module contains modified versions of the `tokio`
//! 0.2 runtimes that are capable of providing `tokio` 0.1 and `tokio`
//! 0.2-compatible runtime services.
//!
//! ## Examples
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
//! # use std::time::{Duration, Instant};
//! use futures_01::future::lazy;
//! use futures_util::compat::*;
//!
//! tokio_compat::run_03(async {
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
//! [futures-compat]: https://rust-lang-nursery.github.io/futures-api-docs/0.3.0-alpha.19/futures/compat/index.html
mod compat;
mod threadpool;

pub use threadpool::{run, run_03, Builder, Runtime};
