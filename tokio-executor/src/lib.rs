//! Tokio execution primitives

#![deny(missing_docs, missing_debug_implementations, warnings)]
#![doc(html_root_url = "https://docs.rs/tokio-executor/0.1")]

extern crate futures;

mod enter;
mod global;
pub mod park;

pub use enter::{enter, Enter, EnterError};
pub use global::{spawn, with_default_executor, DefaultExecutor};

use futures::Future;

/// A trait for types that spawn any future that is `Send`.
///
/// This trait is typically implemented for thread pool type executors
pub trait Executor {
    /// Spawns a future object to run on this executor.
    fn spawn(&mut self, future: Box<Future<Item = (), Error = ()> + Send>)
        -> Result<(), SpawnError>;

    /// Provides a best effort **hint** to whether or not `spawn` will succeed.
    ///
    /// This allows a caller to avoid creating the task if the call to `spawn`
    /// will fail. This is similar to `Sink::poll_ready`, but does not provide
    /// any notification when the state changes nor does it provide a
    /// **guarantee** of what `spawn` will do.
    fn status(&self) -> Result<(), SpawnError> {
        Ok(())
    }
}

/// Errors returned by `Executor::spawn`.
#[derive(Debug)]
pub struct SpawnError {
    is_shutdown: bool,
}

impl SpawnError {
    /// Return a new `SpawnError` reflecting a shutdown executor failure.
    pub fn shutdown() -> Self {
        SpawnError { is_shutdown: true }
    }

    /// Return a new `SpawnError` reflecting an executor at capacity failure.
    pub fn at_capacity() -> Self {
        SpawnError { is_shutdown: false }
    }

    /// Returns `true` if the error reflects a shutdown executor failure.
    pub fn is_shutdown(&self) -> bool {
        self.is_shutdown
    }

    /// Returns `true` if the error reflects an executor at capacity failure.
    pub fn is_at_capacity(&self) -> bool {
        !self.is_shutdown
    }
}
