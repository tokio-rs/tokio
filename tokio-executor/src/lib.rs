#![deny(missing_docs, missing_debug_implementations, warnings)]
#![doc(html_root_url = "https://docs.rs/tokio-executor/0.1.6")]

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
//! [`Executor`]: trait.Executor.html
//! [`enter`]: fn.enter.html
//! [`DefaultExecutor`]: struct.DefaultExecutor.html
//! [`Park`]: park/index.html
//! [`Future::poll`]: https://docs.rs/futures/0.1/futures/future/trait.Future.html#tymethod.poll
//!
//! # Implementing an executor
//!
//! TODO: Dox

extern crate crossbeam_utils;
extern crate futures;

mod enter;
mod error;
mod executor;
mod global;
pub mod park;
mod typed;

pub use enter::{enter, Enter, EnterError};
pub use error::SpawnError;
pub use executor::Executor;
pub use global::{spawn, with_default, DefaultExecutor};
pub use typed::TypedExecutor;
