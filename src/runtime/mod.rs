//! A batteries included runtime for applications using Tokio.
//!
//! Applications using Tokio require some runtime support in order to work:
//!
//! * A [reactor] to drive I/O resources.
//! * An [executor] to execute tasks that use these I/O resources.
//! * A [timer] for scheduling work to run after a set period of time.
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
//! * Run an instance of [`Timer`] **per** thread pool worker thread.
//!
//! The thread pool uses a work-stealing strategy and is configured to start a
//! worker thread for each CPU core available on the system. This tends to be
//! the ideal setup for Tokio applications.
//!
//! A timer per thread pool worker thread is used to minimize the amount of
//! synchronization that is required for working with the timer.
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
//! [timer]: ../timer/index.html
//! [`Runtime`]: struct.Runtime.html
//! [`Reactor`]: ../reactor/struct.Reactor.html
//! [`ThreadPool`]: ../executor/thread_pool/struct.ThreadPool.html
//! [`run`]: fn.run.html
//! [idle]: struct.Runtime.html#method.shutdown_on_idle
//! [`tokio::spawn`]: ../executor/fn.spawn.html
//! [`Timer`]: https://docs.rs/tokio-timer/0.2/tokio_timer/timer/struct.Timer.html

mod builder;
pub mod current_thread;
mod shutdown;
mod task_executor;

pub use self::builder::Builder;
pub use self::shutdown::Shutdown;
pub use self::task_executor::TaskExecutor;

use reactor::{Background, Handle};

use std::io;

use tokio_executor::enter;
use tokio_threadpool as threadpool;

use futures;
use futures::future::Future;

/// Handle to the Tokio runtime.
///
/// The Tokio runtime includes a reactor as well as an executor for running
/// tasks.
///
/// Instances of `Runtime` can be created using [`new`] or [`Builder`]. However,
/// most users will use [`tokio::run`], which uses a `Runtime` internally.
///
/// See [module level][mod] documentation for more details.
///
/// [mod]: index.html
/// [`new`]: #method.new
/// [`Builder`]: struct.Builder.html
/// [`tokio::run`]: fn.run.html
#[derive(Debug)]
pub struct Runtime {
    inner: Option<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// Reactor running on a background thread.
    reactor: Background,

    /// Task execution pool.
    pool: threadpool::ThreadPool,
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
    enter().expect("nested tokio::run")
        .block_on(runtime.shutdown_on_idle())
        .unwrap();
}

impl Runtime {
    /// Create a new runtime instance with default configuration values.
    ///
    /// This results in a reactor, thread pool, and timer being initialized. The
    /// thread pool will not spawn any worker threads until it needs to, i.e.
    /// tasks are scheduled to run.
    ///
    /// Most users will not need to call this function directly, instead they
    /// will use [`tokio::run`](fn.run.html).
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// # Examples
    ///
    /// Creating a new `Runtime` with default configuration values.
    ///
    /// ```
    /// use tokio::runtime::Runtime;
    /// use tokio::prelude::*;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// // Use the runtime...
    ///
    /// // Shutdown the runtime
    /// rt.shutdown_now()
    ///     .wait().unwrap();
    /// ```
    ///
    /// [mod]: index.html
    pub fn new() -> io::Result<Self> {
        Builder::new().build()
    }

    #[deprecated(since = "0.1.5", note = "use `reactor` instead")]
    #[doc(hidden)]
    pub fn handle(&self) -> &Handle {
        self.reactor()
    }

    /// Return a reference to the reactor handle for this runtime instance.
    ///
    /// The returned handle reference can be cloned in order to get an owned
    /// value of the handle. This handle can be used to initialize I/O resources
    /// (like TCP or UDP sockets) that will not be used on the runtime.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// let reactor_handle = rt.reactor().clone();
    ///
    /// // use `reactor_handle`
    /// ```
    pub fn reactor(&self) -> &Handle {
        self.inner().reactor.handle()
    }

    /// Return a handle to the runtime's executor.
    ///
    /// The returned handle can be used to spawn tasks that run on this runtime.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// let executor_handle = rt.executor();
    ///
    /// // use `executor_handle`
    /// ```
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

    /// Run a future to completion on the Tokio runtime.
    ///
    /// This runs the given future on the runtime, blocking until it is
    /// complete, and yielding its resolved result. Any tasks or timers which
    /// the future spawns internally will be executed on the runtime.
    ///
    /// This method should not be called from an asynchronous context.
    ///
    /// # Panics
    ///
    /// This function panics if the executor is at capacity, if the provided
    /// future panics, or if called within an asynchronous execution context.
    pub fn block_on<F, R, E>(&mut self, future: F) -> Result<R, E>
    where
        F: Send + 'static + Future<Item = R, Error = E>,
        R: Send + 'static,
        E: Send + 'static,
    {
        let (tx, rx) = futures::sync::oneshot::channel();
        self.spawn(future.then(move |r| tx.send(r).map_err(|_| unreachable!())));
        rx.wait().unwrap()
    }

    /// Run a future to completion on the Tokio runtime, then wait for all
    /// background futures to complete too.
    ///
    /// This runs the given future on the runtime, blocking until it is
    /// complete, waiting for background futures to complete, and yielding
    /// its resolved result. Any tasks or timers which the future spawns
    /// internally will be executed on the runtime and waited for completion.
    ///
    /// This method should not be called from an asynchronous context.
    ///
    /// # Panics
    ///
    /// This function panics if the executor is at capacity, if the provided
    /// future panics, or if called within an asynchronous execution context.
    pub fn block_on_all<F, R, E>(mut self, future: F) -> Result<R, E>
    where
        F: Send + 'static + Future<Item = R, Error = E>,
        R: Send + 'static,
        E: Send + 'static,
    {
        let res = self.block_on(future);
        self.shutdown_on_idle().wait().unwrap();
        res
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
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime::Runtime;
    /// use tokio::prelude::*;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// // Use the runtime...
    ///
    /// // Shutdown the runtime
    /// rt.shutdown_on_idle()
    ///     .wait().unwrap();
    /// ```
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
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime::Runtime;
    /// use tokio::prelude::*;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// // Use the runtime...
    ///
    /// // Shutdown the runtime
    /// rt.shutdown_now()
    ///     .wait().unwrap();
    /// ```
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
