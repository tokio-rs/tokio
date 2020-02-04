#![deny(missing_docs, missing_debug_implementations)]
#![doc(html_root_url = "https://docs.rs/tokio-executor/0.1.10")]

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
//! This crate provides traits and utilities that are necessary for building an
//! executor, including:
//!
//! * The [`Executor`] trait spawns future object onto an executor.
//!
//! * The [`TypedExecutor`] trait spawns futures of a specific type onto an
//!   executor. This is used to be generic over executors that spawn futures
//!   that are either `Send` or `!Send` or implement executors that apply to
//!   specific futures.
//!
//! * [`enter`] marks that the current thread is entering an execution
//!   context. This prevents a second executor from accidentally starting from
//!   within the context of one that is already running.
//!
//! * [`DefaultExecutor`] spawns tasks onto the default executor for the current
//!   context.
//!
//! * [`Park`] abstracts over blocking and unblocking the current thread.
//!
//! # Implementing an executor
//!
//! Executors should always implement `TypedExecutor`. This usually is the bound
//! that applications and libraries will use when generic over an executor. See
//! the [trait documentation][`TypedExecutor`] for more details.
//!
//! If the executor is able to spawn all futures that are `Send`, then the
//! executor should also implement the `Executor` trait. This trait is rarely
//! used directly by applications and libraries. Instead, `tokio::spawn` is
//! configured to dispatch to type that implements `Executor`.
//!
//! [`Executor`]: trait.Executor.html
//! [`TypedExecutor`]: trait.TypedExecutor.html
//! [`enter`]: fn.enter.html
//! [`DefaultExecutor`]: struct.DefaultExecutor.html
//! [`Park`]: park/index.html
//! [`Future::poll`]: https://docs.rs/futures/0.1/futures/future/trait.Future.html#tymethod.poll

extern crate crossbeam_utils;
extern crate futures;

mod enter;
mod error;
mod executor;
mod global;
pub mod park;
mod typed;

pub use enter::{enter, exit, Enter, EnterError};
pub use error::SpawnError;
pub use executor::Executor;
pub use global::{set_default, spawn, with_default, DefaultExecutor, DefaultGuard};
pub use typed::TypedExecutor;
