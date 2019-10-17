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

pub use self::builder::Builder;
pub use self::runtime::{Handle, RunError, Runtime};
pub use tokio_executor::current_thread::spawn;
pub use tokio_executor::current_thread::TaskExecutor;

#[cfg(test)]
mod tests;
