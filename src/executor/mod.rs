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

#[deprecated(
    since = "0.1.8",
    note = "use tokio-current-thread crate or functions in tokio::runtime::current_thread instead",
)]
#[doc(hidden)]
pub mod current_thread;

#[deprecated(since = "0.1.8", note = "use tokio-threadpool crate instead")]
#[doc(hidden)]
/// Re-exports of [`tokio-threadpool`], deprecated in favor of the crate.
///
/// [`tokio-threadpool`]: https://docs.rs/tokio-threadpool/0.1
pub mod thread_pool {
    pub use tokio_threadpool::{
        Builder,
        Sender,
        Shutdown,
        ThreadPool,
    };
}

pub use tokio_executor::{Executor, DefaultExecutor, SpawnError};

use futures::{Future, IntoFuture};
use futures::future::{self, FutureResult};

/// Return value from the `spawn` function.
///
/// Currently this value doesn't actually provide any functionality. However, it
/// provides a way to add functionality later without breaking backwards
/// compatibility.
///
/// This also implements `IntoFuture` so that it can be used as the return value
/// in a `for_each` loop.
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
/// ```rust
/// # extern crate tokio;
/// # extern crate futures;
/// # use futures::{Future, Stream};
/// use tokio::net::TcpListener;
///
/// # fn process<T>(_: T) -> Box<Future<Item = (), Error = ()> + Send> {
/// # unimplemented!();
/// # }
/// # fn dox() {
/// # let addr = "127.0.0.1:8080".parse().unwrap();
/// let listener = TcpListener::bind(&addr).unwrap();
///
/// let server = listener.incoming()
///     .map_err(|e| println!("error = {:?}", e))
///     .for_each(|socket| {
///         tokio::spawn(process(socket))
///     });
///
/// tokio::run(server);
/// # }
/// # pub fn main() {}
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
where F: Future<Item = (), Error = ()> + 'static + Send
{
    ::tokio_executor::spawn(f);
    Spawn(())
}

/// Spawns a future on the default executor, delaying creation
/// of the future until it actually runs.
///
/// This method is almost identical to [`spawn`], except that
/// it takes a closure that will be called to create a future.
///
/// Unlike [`spawn`], this method requires that the *function*
/// be [`Send`]. not the future itself. This allows the future
/// returned by the function to use non-Send types like [`Rc`],
/// which would normally not be allowed.
///
/// The provided closure will be invoked just before the created
/// task is run for the first time. The invocation will occur
/// on the thread that will actually be running the future.
/// If a threadpool is in use, this will be the worker thread
/// that is responsible for driving the future to completion.
/// If a single-threaded executor is use, then the closure
/// will be invoked on the same that that calls spawn_lazy
///
/// # Examples
///
/// In this example, a server is started and `spawn_lazy` is used to start a new task
/// that processes each received connection. The task uses an Rc,
/// which would not be possible with `spawn`.
///
/// ```rust
/// # extern crate tokio;
/// # extern crate futures;
/// # use futures::{Future, Stream};
/// use tokio::net::TcpListener;
/// use std::rc::Rc;
/// use std::cell::RefCell;
///
/// # fn process<T>(_: T, rc: Rc<RefCell<bool>>) -> Box<Future<Item = (), Error = ()> + Send> {
/// # unimplemented!();
/// # }
/// # fn dox() {
/// # let addr = "127.0.0.1:8080".parse().unwrap();
/// let listener = TcpListener::bind(&addr).unwrap();
///
/// let server = listener.incoming()
///     .map_err(|e| println!("error = {:?}", e))
///     .for_each(|socket| {
///         tokio::spawn_lazy(|| {
///             let rc = Rc::new(RefCell::new(true));;
///             process(socket, rc);
///             Box::new(futures::future::ok(())) as Box<Future<Item = (), Error = ()>>
///         })
///     });
///
/// tokio::run(server);
/// # }
/// # pub fn main() {}
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

pub fn spawn_lazy<F>(f: F) -> Spawn
where F: FnOnce() -> Box<Future<Item = (), Error = ()>> + Send + 'static
{
    ::tokio_executor::spawn_lazy(f);
    Spawn(())
}

impl IntoFuture for Spawn {
    type Future = FutureResult<(), ()>;
    type Item = ();
    type Error = ();

    fn into_future(self) -> Self::Future {
        future::ok(())
    }
}
