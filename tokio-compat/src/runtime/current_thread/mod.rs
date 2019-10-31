//! A compatibility implementation that runs everything on the current thread.
//!
//! [`current_thread::Runtime`][rt] is similar to the primary
//! [compatibility `Runtime`][concurrent-rt] except that it runs all components
//! on the current  thread instead of using a thread pool. This means that it is
//! able to spawn futures that do not implement `Send`.
//!
//! Same as the default [`current_thread::Runtime`][default-rt] in the main
//! `tokio` crate, the [`tokio_compat::current_thread::Runtime`][rt] includes:
//!
//! * A [reactor] to drive I/O resources.
//! * An [executor] to execute tasks that use these I/O resources.
//! * A [timer] for scheduling work to run after a set period of time.
//!
//! Unlike the default `current_thread::Runtime`, however, the `tokio_compat`
//! version must spawn an additional background thread to run the `tokio` 0.1
//! [`Reactor`][reactor-01] and [`Timer`][timer-01]. This is necessary to
//! support legacy tasks, as the main thread is already running a `tokio` 0.2
//! `Reactor` and `Timer`.
//!
//! Note that [`current_thread::Runtime`][rt] does not implement `Send` itself
//! and cannot be safely moved to other threads
//!
//! # Spawning from other threads
//!
//! While [`current_thread::Runtime`][rt] does not implement `Send` and cannot
//! safely be moved to other threads, it provides a `Handle` that can be sent
//! to other threads and allows to spawn new tasks from there.
//!
//! For example:
//!
//! ```
//! use tokio_compat::runtime::current_thread::Runtime;
//! use std::thread;
//!
//! let runtime = Runtime::new().unwrap();
//! let handle = runtime.handle();
//!
//! thread::spawn(move || {
//!     // Spawn a `futures` 0.1 task on the other thread's runtime.
//!     let _ = handle.spawn(futures_01::future::lazy(|| {
//!         println!("hello from futures 0.1!");
//!         Ok(())
//!     }));
//!
//!     // Spawn a `std::future` task on the other thread's runtime.
//!     let _ = handle.spawn_std(async {
//!         println!("hello from std::future!");
//!     });
//! }).join().unwrap();
//! ```
//!
//! # Examples
//!
//! Creating a new `Runtime` and running a future `f` until its completion and
//! returning its result.
//!
//! ```
//! use tokio_compat::runtime::current_thread::Runtime;
//!
//! let runtime = Runtime::new().unwrap();
//!
//! // Use the runtime...
//! // runtime.block_on(f); // where f is a future
//! ```
//!
//! [rt]: struct.Runtime.html
//! [concurrent-rt]: ../struct.Runtime.html
//! [default-rt]:
//!     https://docs.rs/tokio/0.2.0-alpha.6/tokio/runtime/current_thread/struct.Runtime.html
//! [chan]: https://docs.rs/futures/0.1/futures/sync/mpsc/fn.channel.html
//! [reactor]: ../../reactor/struct.Reactor.html
//! [executor]: https://tokio.rs/docs/internals/runtime-model/#executors
//! [timer]: ../../timer/index.html
//! [timer-01]: https://docs.rs/tokio/0.1.22/tokio/timer/index.html
//! [reactor-01]: https://docs.rs/tokio/0.1.22/tokio/reactor/struct.Reactor.html
use super::compat;

mod builder;
mod runtime;
mod task_executor;

pub use self::builder::Builder;
pub use self::runtime::{Handle, RunError, Runtime};
pub use self::task_executor::TaskExecutor;

use futures_01::future::Future as Future01;
use futures_util::{compat::Future01CompatExt, FutureExt};
use std::future::Future;

/// Run the provided `futures` 0.1 future to completion using a runtime running on the current thread.
///
/// This first creates a new [`Runtime`], and calls [`Runtime::block_on`] with the provided future,
/// which blocks the current thread until the provided future completes. It then calls
/// [`Runtime::run`] to wait for any other spawned futures to resolve.
pub fn block_on_all<F>(future: F) -> Result<F::Item, F::Error>
where
    F: Future01,
{
    block_on_all_std(future.compat())
}

/// Run the provided `std::future` future to completion using a runtime running on the current thread.
///
/// This first creates a new [`Runtime`], and calls [`Runtime::block_on`] with the provided future,
/// which blocks the current thread until the provided future completes. It then calls
/// [`Runtime::run`] to wait for any other spawned futures to resolve.
pub fn block_on_all_std<F>(future: F) -> F::Output
where
    F: Future,
{
    let mut r = Runtime::new().expect("failed to start runtime on current thread");
    let v = r.block_on_std(future);
    r.run().expect("failed to resolve remaining futures");
    v
}

/// Start a current-thread runtime using the supplied `futures` 0.1 future to bootstrap execution.
///
/// # Panics
///
/// This function panics if called from the context of an executor.
pub fn run<F>(future: F)
where
    F: Future01<Item = (), Error = ()> + 'static,
{
    run_std(future.compat().map(|_| ()))
}

/// Start a current-thread runtime using the supplied `std::future` ture to bootstrap execution.
///
/// # Panics
///
/// This function panics if called from the context of an executor.
pub fn run_std<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    let mut r = Runtime::new().expect("failed to start runtime on current thread");
    r.spawn_std(future);
    r.run().expect("failed to resolve remaining futures");
}

#[cfg(test)]
mod tests;
