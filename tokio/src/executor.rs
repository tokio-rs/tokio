//! Task execution utilities.
//!
//! In the Tokio execution model, futures are lazy. When a future is created, no
//! work is performed. In order for the work defined by the future to happen,
//! the future must be submitted to an executor. A future that is submitted to
//! an executor is called a "task".
//!
//! The executor is responsible for ensuring that [`Future::poll`] is
//! called whenever the task is [notified]. Notification happens when the
//! internal state of a task transitions from "not ready" to ready. For
//! example, a socket might have received data and a call to `read` will now be
//! able to succeed.
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
//! [`spawn`]: fn.spawn.html

use std::future::Future;
pub use tokio_executor::{DefaultExecutor, Executor, SpawnError, TypedExecutor};

/// Return value from the `spawn` function.
///
/// Currently this value doesn't actually provide any functionality. However, it
/// provides a way to add functionality later without breaking backwards
/// compatibility.
///
/// See [`spawn`] for more details.
///
/// [`spawn`]: fn.spawn.html
#[derive(Debug)]
pub struct Spawn(());

/// Spawns a future on the default executor.
///
/// In order for a future to do work, it must be spawned on an executor. The
/// `spawn` function is the easiest way to do this. It spawns a future on the
/// [default executor] for the current execution context (tracked using a
/// thread-local variable).
///
/// The default executor is **usually** a thread pool.
///
/// # Examples
///
/// In this example, a server is started and `spawn` is used to start a new task
/// that processes each received connection.
///
/// ```
/// use tokio::net::TcpListener;
///
/// # async fn process<T>(t: T) {}
/// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
/// let addr = "127.0.0.1:8080".parse()?;
/// let mut listener = TcpListener::bind(&addr).unwrap();
///
/// loop {
///     let (socket, _) = listener.accept().await?;
///
///     tokio::spawn(async move {
///         // Process each socket concurrently.
///         process(socket).await
///     });
/// }
/// # Ok(())
/// # }
/// ```
///
/// [default executor]: struct.DefaultExecutor.html
///
/// # Panics
///
/// This function will panic if the default executor is not set or if spawning
/// onto the default executor returns an error. To avoid the panic, use
/// [`DefaultExecutor`].
///
/// [`DefaultExecutor`]: struct.DefaultExecutor.html
pub fn spawn<F>(f: F) -> Spawn
where
    F: Future<Output = ()> + 'static + Send,
{
    ::tokio_executor::spawn(f);
    Spawn(())
}
