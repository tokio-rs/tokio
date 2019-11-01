//! Task execution related traits and utilities.
//!
//! In the Tokio execution model, futures are lazy. When a future is created, no
//! work is performed. In order for the work defined by the future to happen,
//! the future must be submitted to an executor. A future that is submitted to
//! an executor is called a "task".
//!
//! The executor is responsible for ensuring that [`Future::poll`] is called
//! whenever the task is notified. Notification happens when the internal
//! state of a task transitions from *not ready* to *ready*. For example, a
//! socket might have received data and a call to `read` will now be able to
//! succeed.
//!
//! The specific strategy used to manage the tasks is left up to the
//! executor. There are two main flavors of executors: single-threaded and
//! multi-threaded. Tokio provides implementation for both of these in the
//! [`runtime`] module.
//!
//! # `Executor` trait.
//!
//! This module provides the [`Executor`] trait (re-exported from
//! [`tokio-executor`]), which describes the API that all executors must
//! implement.
//!
//! A free [`spawn`] function is provided that allows spawning futures onto the
//! default executor (tracked via a thread-local variable) without referencing a
//! handle. It is expected that all executors will set a value for the default
//! executor. This value will often be set to the executor itself, but it is
//! possible that the default executor might be set to a different executor.
//!
//! For example, a single threaded executor might set the default executor to a
//! thread pool instead of itself, allowing futures to spawn new tasks onto the
//! thread pool when those tasks are `Send`.
//!
//! [`Future::poll`]: https://docs.rs/futures/0.1/futures/future/trait.Future.html#tymethod.poll
//! [notified]: https://docs.rs/futures/0.1/futures/executor/trait.Notify.html#tymethod.notify
//! [`runtime`]: ../runtime/index.html
//! [`tokio-executor`]: https://docs.rs/tokio-executor/0.1
//! [`Executor`]: trait.Executor.html
//! [`spawn`]: fn.spawn.html#[cfg(all(test, loom))]

// At the top due to macros
#[cfg(test)]
#[macro_use]
mod tests;

mod enter;
pub use self::enter::{enter, exit, Enter, EnterError};

mod global;
pub use self::global::spawn;

pub(crate) mod loom;

pub mod park;

#[cfg(feature = "rt-current-thread")]
mod task;
#[cfg(feature = "rt-current-thread")]
pub use self::task::{JoinError, JoinHandle};

#[cfg(feature = "rt-full")]
mod util;

#[cfg(all(not(feature = "blocking"), feature = "rt-full"))]
mod blocking;
#[cfg(feature = "blocking")]
pub mod blocking;

#[cfg(feature = "rt-current-thread")]
pub(crate) mod current_thread;

#[cfg(feature = "rt-full")]
pub mod thread_pool;

pub use futures_util::future::RemoteHandle;
