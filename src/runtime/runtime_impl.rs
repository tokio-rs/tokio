//! A batteries included runtime for applications using Tokio.
//!
//! Applications using Tokio require some runtime support in order to work:
//!
//! * A [reactor] to drive I/O resources.
//! * An [executor] to execute tasks that use these I/O resources.
//!
//! While it is possible to setup each component manually, this involves a bunch
//! of boilerplate.
//!
//! [`Runtime`] bundles all of these various runtime components into a single
//! handle that can be started and shutdown together, eliminating the necessary
//! boilerplate to run a Tokio application.
//!
//! Most applications wont need to use [`Runtime`] directly. Instead, they will
//! use the [`run`] function, which uses [`Runtime`] under the hood.
//!
//! Creating a [`Runtime`] does the following:
//!
//! * Spawn a background thread running a [`Reactor`] instance.
//! * Start a [`ThreadPool`] for executing futures.
//!
//! The thread pool uses a work-stealing strategy and is configured to start a
//! worker thread for each CPU core available on the system. This tends to be
//! the ideal setup for Tokio applications.
//!
//! # Usage
//!
//! Most applications will use the [`run`] function. This takes a future to
//! "seed" the application, blocking the thread until the runtime becomes
//! [idle].
//!
//! ```rust
//! # extern crate tokio;
//! # extern crate futures;
//! # use futures::{Future, Stream};
//! use tokio::net::TcpListener;
//!
//! # fn process<T>(_: T) -> Box<Future<Item = (), Error = ()> + Send> {
//! # unimplemented!();
//! # }
//! # fn dox() {
//! # let addr = "127.0.0.1:8080".parse().unwrap();
//! let listener = TcpListener::bind(&addr).unwrap();
//!
//! let server = listener.incoming()
//!     .map_err(|e| println!("error = {:?}", e))
//!     .for_each(|socket| {
//!         tokio::spawn(process(socket))
//!     });
//!
//! tokio::run(server);
//! # }
//! # pub fn main() {}
//! ```
//!
//! In this function, the `run` function blocks until the runtime becomes idle.
//! See [`shutdown_on_idle`][idle] for more shutdown details.
//!
//! From within the context of the runtime, additional tasks are spawned using
//! the [`tokio::spawn`] function. Futures spawned using this function will be
//! executed on the same thread pool used by the [`Runtime`].
//!
//! A [`Runtime`] instance can also be used directly.
//!
//! ```rust
//! # extern crate tokio;
//! # extern crate futures;
//! # use futures::{Future, Stream};
//! use tokio::runtime::Runtime;
//! use tokio::net::TcpListener;
//!
//! # fn process<T>(_: T) -> Box<Future<Item = (), Error = ()> + Send> {
//! # unimplemented!();
//! # }
//! # fn dox() {
//! # let addr = "127.0.0.1:8080".parse().unwrap();
//! let listener = TcpListener::bind(&addr).unwrap();
//!
//! let server = listener.incoming()
//!     .map_err(|e| println!("error = {:?}", e))
//!     .for_each(|socket| {
//!         tokio::spawn(process(socket))
//!     });
//!
//! // Create the runtime
//! let mut rt = Runtime::new().unwrap();
//!
//! // Spawn the server task
//! rt.spawn(server);
//!
//! // Wait until the runtime becomes idle and shut it down.
//! rt.shutdown_on_idle()
//!     .wait().unwrap();
//! # }
//! # pub fn main() {}
//! ```
//!
//! [reactor]: ../reactor/struct.Reactor.html
//! [executor]: https://tokio.rs/docs/getting-started/runtime-model/#executors
//! [`Runtime`]: struct.Runtime.html
//! [`ThreadPool`]: ../executor/thread_pool/struct.ThreadPool.html
//! [`run`]: fn.run.html
//! [idle]: struct.Runtime.html#method.shutdown_on_idle
//! [`tokio::spawn`]: ../executor/fn.spawn.html

use runtime::builder::Builder;
use runtime::inner::Inner;
use runtime::shutdown::Shutdown;
use runtime::task_executor::TaskExecutor;

use reactor::Handle;

use tokio_threadpool as threadpool;
use futures::future::Future;

use std::io;

#[cfg(feature = "unstable-futures")]
use futures2;

/// Handle to the Tokio runtime.
///
/// The Tokio runtime includes a reactor as well as an executor for running
/// tasks.
///
/// See [module level][mod] documentation for more details.
///
/// [mod]: index.html
#[derive(Debug)]
pub struct Runtime {
    pub(super) inner: Option<Inner>,
}

// ===== impl Runtime =====

/// Start the Tokio runtime using the supplied future to bootstrap execution.
///
/// This function is used to bootstrap the execution of a Tokio application. It
/// does the following:
///
/// * Start the Tokio runtime using a default configuration.
/// * Spawn the given future onto the thread pool.
/// * Block the current thread until the runtime shuts down.
///
/// Note that the function will not return immediately once `future` has
/// completed. Instead it waits for the entire runtime to become idle.
///
/// See the [module level][mod] documentation for more details.
///
/// # Examples
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
/// # Panics
///
/// This function panics if called from the context of an executor.
///
/// [mod]: ../index.html
pub fn run<F>(future: F)
where F: Future<Item = (), Error = ()> + Send + 'static,
{
    let mut runtime = Runtime::new().unwrap();
    runtime.spawn(future);
    runtime.shutdown_on_idle().wait().unwrap();
}

/// Start the Tokio runtime using the supplied future to bootstrap execution.
///
/// Identical to `run` but works with futures 0.2-style futures.
#[cfg(feature = "unstable-futures")]
pub fn run2<F>(future: F)
    where F: futures2::Future<Item = (), Error = futures2::Never> + Send + 'static,
{
    let mut runtime = Runtime::new().unwrap();
    runtime.spawn2(future);
    runtime.shutdown_on_idle().wait().unwrap();
}

impl Runtime {
    /// Create a new runtime instance with default configuration values.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    pub fn new() -> io::Result<Self> {
        Builder::new()
            .threadpool_builder(
                threadpool::Builder::new()
                    .name_prefix("tokio-runtime-worker-")
                    .clone()
            )
            .build()
    }

    /// Return a reference to the reactor handle for this runtime instance.
    pub fn handle(&self) -> &Handle {
        self.inner().reactor.handle()
    }

    /// Return a handle to the runtime's executor.
    pub fn executor(&self) -> TaskExecutor {
        let inner = self.inner().pool.sender().clone();
        TaskExecutor { inner }
    }

    /// Spawn a future onto the Tokio runtime.
    ///
    /// This spawns the given future onto the runtime's executor, usually a
    /// thread pool. The thread pool is then responsible for polling the future
    /// until it completes.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate tokio;
    /// # extern crate futures;
    /// # use futures::{future, Future, Stream};
    /// use tokio::runtime::Runtime;
    ///
    /// # fn dox() {
    /// // Create the runtime
    /// let mut rt = Runtime::new().unwrap();
    ///
    /// // Spawn a future onto the runtime
    /// rt.spawn(future::lazy(|| {
    ///     println!("now running on a worker thread");
    ///     Ok(())
    /// }));
    /// # }
    /// # pub fn main() {}
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn<F>(&mut self, future: F) -> &mut Self
    where F: Future<Item = (), Error = ()> + Send + 'static,
    {
        self.inner_mut().pool.sender().spawn(future).unwrap();
        self
    }

    /// Spawn a futures 0.2-style future onto the Tokio runtime.
    ///
    /// Otherwise identical to `spawn`
    #[cfg(feature = "unstable-futures")]
    pub fn spawn2<F>(&mut self, future: F) -> &mut Self
        where F: futures2::Future<Item = (), Error = futures2::Never> + Send + 'static,
    {
        futures2::executor::Executor::spawn(
            self.inner_mut().pool.sender_mut(), Box::new(future)
        ).unwrap();
        self
    }

    /// Signals the runtime to shutdown once it becomes idle.
    ///
    /// Returns a future that completes once the shutdown operation has
    /// completed.
    ///
    /// This function can be used to perform a graceful shutdown of the runtime.
    ///
    /// The runtime enters an idle state once **all** of the following occur.
    ///
    /// * The thread pool has no tasks to execute, i.e., all tasks that were
    ///   spawned have completed.
    /// * The reactor is not managing any I/O resources.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    pub fn shutdown_on_idle(mut self) -> Shutdown {
        let inner = self.inner.take().unwrap();

        let inner = Box::new({
            let pool = inner.pool;
            let reactor = inner.reactor;

            pool.shutdown_on_idle().and_then(|_| {
                reactor.shutdown_on_idle()
            })
        });

        Shutdown { inner }
    }

    /// Signals the runtime to shutdown immediately.
    ///
    /// Returns a future that completes once the shutdown operation has
    /// completed.
    ///
    /// This function will forcibly shutdown the runtime, causing any
    /// in-progress work to become canceled. The shutdown steps are:
    ///
    /// * Drain any scheduled work queues.
    /// * Drop any futures that have not yet completed.
    /// * Drop the reactor.
    ///
    /// Once the reactor has dropped, any outstanding I/O resources bound to
    /// that reactor will no longer function. Calling any method on them will
    /// result in an error.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    pub fn shutdown_now(mut self) -> Shutdown {
        let inner = self.inner.take().unwrap();
        Shutdown::shutdown_now(inner)
    }

    fn inner(&self) -> &Inner {
        self.inner.as_ref().unwrap()
    }

    fn inner_mut(&mut self) -> &mut Inner {
        self.inner.as_mut().unwrap()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            let shutdown = Shutdown::shutdown_now(inner);
            let _ = shutdown.wait();
        }
    }
}
