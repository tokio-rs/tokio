//! Task execution related traits and utilities.
//!
//! In the Tokio execution model, futures are lazy. When a future is created, no
//! work is performed. In order for the work defined by the future to happen,
//! the future must be submitted to an executor. A future that is submitted to
//! an executor is called a "task".
//!
//! The executor is responsible for ensuring that [`Future::poll`] is called
//! whenever the task is [notified]. Notification happens when the internal
//! state of a task transitions from "not ready" to ready. For example, a socket
//! might have received data and a call to `read` will now be able to succeed.
//!
//! This crate provides traits and utilities that are necessary for building an
//! executor, including:
//!
//! * The [`Executor`] trait describes the API for spawning a future onto an
//!   executor.
//!
//! * [`enter`] marks that the the current thread is entering an execution
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

#![deny(missing_docs, missing_debug_implementations, warnings)]
#![doc(html_root_url = "https://docs.rs/tokio-executor/0.1.0")]

extern crate futures;

#[cfg(feature = "unstable-futures")]
extern crate futures2;

mod enter;
mod global;
pub mod park;

pub use enter::{enter, Enter, EnterError};
pub use global::{spawn, with_default, DefaultExecutor};

#[cfg(feature = "unstable-futures")]
pub use global::spawn2;

use futures::Future;

/// A value that executes futures.
///
/// The [`spawn`] function is used to submit a future to an executor. Once
/// submitted, the executor takes ownership of the future and becomes
/// responsible for driving the future to completion.
///
/// The strategy employed by the executor to handle the future is less defined
/// and is left up to the `Executor` implementation. The `Executor` instance is
/// expected to call [`poll`] on the future once it has been notified, however
/// the "when" and "how" can vary greatly.
///
/// For example, the executor might be a thread pool, in which case a set of
/// threads have already been spawned up and the future is inserted into a
/// queue. A thread will acquire the future and poll it.
///
/// The `Executor` trait is only for futures that **are** `Send`. These are most
/// common. There currently is no trait that describes executors that operate
/// entirely on the current thread (i.e., are able to spawn futures that are not
/// `Send`). Note that single threaded executors can still implement `Executor`,
/// but only futures that are `Send` can be spawned via the trait.
///
/// # Errors
///
/// The [`spawn`] function returns `Result` with an error type of `SpawnError`.
/// This error type represents the reason that the executor was unable to spawn
/// the future. The two current represented scenarios are:
///
/// * An executor being at capacity or full. As such, the executor is not able
///   to accept a new future. This error state is expected to be transient.
/// * An executor has been shutdown and can no longer accept new futures. This
///   error state is expected to be permanent.
///
/// If a caller encounters an at capacity error, the caller should try to shed
/// load. This can be as simple as dropping the future that was spawned.
///
/// If the caller encounters a shutdown error, the caller should attempt to
/// gracefully shutdown.
///
/// # Examples
///
/// ```rust
/// # extern crate futures;
/// # extern crate tokio_executor;
/// # use tokio_executor::Executor;
/// # fn docs(my_executor: &mut Executor) {
/// use futures::future::lazy;
/// my_executor.spawn(Box::new(lazy(|| {
///     println!("running on the executor");
///     Ok(())
/// }))).unwrap();
/// # }
/// # fn main() {}
/// ```
///
/// [`spawn`]: #tymethod.spawn
/// [`poll`]: https://docs.rs/futures/0.1/futures/future/trait.Future.html#tymethod.poll
pub trait Executor {
    /// Spawns a future object to run on this executor.
    ///
    /// `future` is passed to the executor, which will begin running it. The
    /// future may run on the current thread or another thread at the discretion
    /// of the `Executor` implementation.
    ///
    /// # Panics
    ///
    /// Implementors are encouraged to avoid panics. However, a panic is
    /// permitted and the caller should check the implementation specific
    /// documentation for more details on possible panics.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate futures;
    /// # extern crate tokio_executor;
    /// # use tokio_executor::Executor;
    /// # fn docs(my_executor: &mut Executor) {
    /// use futures::future::lazy;
    /// my_executor.spawn(Box::new(lazy(|| {
    ///     println!("running on the executor");
    ///     Ok(())
    /// }))).unwrap();
    /// # }
    /// # fn main() {}
    /// ```
    fn spawn(&mut self, future: Box<Future<Item = (), Error = ()> + Send>)
             -> Result<(), SpawnError>;

    /// Like `spawn`, but compatible with futures 0.2
    #[cfg(feature = "unstable-futures")]
    fn spawn2(&mut self, future: Box<futures2::Future<Item = (), Error = futures2::Never> + Send>)
             -> Result<(), futures2::executor::SpawnError>;

    /// Provides a best effort **hint** to whether or not `spawn` will succeed.
    ///
    /// This function may return both false positives **and** false negatives.
    /// If `status` returns `Ok`, then a call to `spawn` will *probably*
    /// succeed, but may fail. If `status` returns `Err`, a call to `spawn` will
    /// *probably* fail, but may succeed.
    ///
    /// This allows a caller to avoid creating the task if the call to `spawn`
    /// has a high likelihood of failing.
    ///
    /// # Panics
    ///
    /// This function must not panic. Implementors must ensure that panics do
    /// not happen.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate futures;
    /// # extern crate tokio_executor;
    /// # use tokio_executor::Executor;
    /// # fn docs(my_executor: &mut Executor) {
    /// use futures::future::lazy;
    ///
    /// if my_executor.status().is_ok() {
    ///     my_executor.spawn(Box::new(lazy(|| {
    ///         println!("running on the executor");
    ///         Ok(())
    ///     }))).unwrap();
    /// } else {
    ///     println!("the executor is not in a good state");
    /// }
    /// # }
    /// # fn main() {}
    /// ```
    fn status(&self) -> Result<(), SpawnError> {
        Ok(())
    }
}

/// Errors returned by `Executor::spawn`.
///
/// Spawn errors should represent relatively rare scenarios. Currently, the two
/// scenarios represented by `SpawnError` are:
///
/// * An executor being at capacity or full. As such, the executor is not able
///   to accept a new future. This error state is expected to be transient.
/// * An executor has been shutdown and can no longer accept new futures. This
///   error state is expected to be permanent.
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
