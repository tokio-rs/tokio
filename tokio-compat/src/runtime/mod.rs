//! Runtimes compatible with both `tokio` 0.1 and `tokio` 0.2 futures.
//!
//! This module is similar to the [`tokio::runtime`] module, with one
//! key difference: the runtimes in this crate are capable of executing
//! both `futures` 0.1 futures that use the `tokio` 0.1 runtime services
//! (i.e. `timer`, `reactor`, and `executor`), **and** `std::future`
//! futures that use the `tokio` 0.2 runtime services.
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
//! 0.2 `Runtime` and `current_thread::Runtime` that are capable of providing
//! `tokio` 0.1 and `tokio` 0.2-compatible runtime services.
//!
//! Creating a [`Runtime`] does the following:
//!
//! * Spawn a background thread running a [`Reactor`] instance.
//! * Start a [`ThreadPool`] for executing futures.
//! * Run an instance of [`Timer`] **per** thread pool worker thread.
//! * Run a **single** `tokio` 0.1 [`Reactor`][reactor-01] and
//!   [`Timer`][timer-01] on a background thread, for legacy tasks.
//!
//! Legacy `futures` 0.1 tasks will be executed by the `tokio` 0.2 thread pool
//! workers, alongside `std::future` tasks. However, they will use the timer and
//! reactor provided by the compatibility background thread.
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
//! ## Notes
//!
//! In order to allow drop-in compatibility for legacy codebases using
//! `tokio` 0.1, the  [`run`], [`spawn`], and [`block_on`] methods provided by the
//! compatibility runtimes take `futures` 0.1 futures. This allows the
//! compatibility runtimes to replace the `tokio` 0.1 runtimes in those codebases
//! without requiring changes to unrelated code. The compatibility runtimes
//! _also_ provide `std::future`-compatible versions of these methods, named
//! [`run_std`], [`spawn_std`], and [`block_on_std`].
//!
//! Also, please note that the compatibility thread pool runtime does **not**
//! currently support the `tokio` 0.1 [`tokio_threadpool::blocking][blocking]
//! API. Calls to the legacy version of `blocking` made on the compatibility
//! runtime will currently fail. In the future, `tokio-compat` will allow
//! transparently replacing legacy `blocking` with the `tokio` 0.2 blocking
//! APIs, but in the meantime, it will be necessary to convert this code to call
//! into the `tokio` 0.2 version of `blocking`.
//!
//! [`tokio::runtime`]: https://docs.rs/tokio/0.2.0-alpha.6/tokio/runtime/index.html
//! [futures-compat]: https://rust-lang-nursery.github.io/futures-api-docs/0.3.0-alpha.19/futures/compat/index.html
//! [`Timer`]: https://docs.rs/tokio/0.2.0-alpha.6/tokio/timer/index.html
//! [`Runtime`]: struct.Runtime.html
//! [`Reactor`]:https://docs.rs/tokio/0.2.0-alpha.6/tokio/reactor/struct.Reactor.html
//! [timer-01]: https://docs.rs/tokio/0.1.22/tokio/timer/index.html
//! [reactor-01]: https://docs.rs/tokio/0.1.22/tokio/reactor/struct.Reactor.html
//! [`ThreadPool`]: https://docs.rs/tokio-executor/0.2.0-alpha.2/tokio_executor/threadpool/struct.ThreadPool.html
//! [`run`]: struct.Runtime.html#method.run
//! [`spawn`]: struct.Runtime.html#method.spawn
//! [`block_on`]: struct.Runtime.html#method.block_on
//! [`run_std`]: struct.Runtime.html#method.run_std
//! [`spawn_std`]: struct.Runtime.html#method.spawn_std
//! [`block_on_std`]: struct.Runtime.html#method.spawn_std
//! [blocking]: https://docs.rs/tokio-threadpool/0.1.16/tokio_threadpool/fn.blocking.html
mod compat;
pub mod current_thread;
#[cfg(feature = "rt-full")]
mod threadpool;

#[cfg(feature = "rt-full")]
pub use threadpool::{run, run_std, Builder, Runtime, TaskExecutor};
