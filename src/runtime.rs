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

use reactor::{Reactor, Handle, Background};

use tokio_threadpool::{self as threadpool, ThreadPool, Sender};
use futures::Poll;
use futures::future::{self, Future};

use std::{fmt, io};

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
    inner: Option<Inner>,
}

/// Executes futures on the runtime
///
/// All futures spawned using this executor will be submitted to the associated
/// Runtime's executor. This executor is usually a thread pool.
///
/// For more details, see the [module level](index.html) documentation.
#[derive(Debug, Clone)]
pub struct TaskExecutor {
    inner: Sender,
}

/// A future that resolves when the Tokio `Runtime` is shut down.
pub struct Shutdown {
    inner: Box<Future<Item = (), Error = ()> + Send>,
}

#[derive(Debug)]
struct Inner {
    /// Reactor running on a background thread.
    reactor: Background,

    /// Task execution pool.
    pool: ThreadPool,
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

impl Runtime {
    /// Create a new runtime instance with default configuration values.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    pub fn new() -> io::Result<Self> {
        // Spawn a reactor on a background thread.
        let reactor = Reactor::new()?.background()?;

        // Get a handle to the reactor.
        let handle = reactor.handle().clone();

        let pool = threadpool::Builder::new()
            .around_worker(move |w, enter| {
                ::tokio_reactor::with_default(&handle, enter, |_| {
                    w.run();
                });
            })
            .build();

        Ok(Runtime {
            inner: Some(Inner {
                reactor,
                pool,
            }),
        })
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

        let inner = Box::new({
            let pool = inner.pool;
            let reactor = inner.reactor;

            pool.shutdown_now().and_then(|_| {
                reactor.shutdown_now()
            })
        });

        Shutdown { inner }
    }

    fn inner(&self) -> &Inner {
        self.inner.as_ref().unwrap()
    }

    fn inner_mut(&mut self) -> &mut Inner {
        self.inner.as_mut().unwrap()
    }
}

// ===== impl TaskExecutor =====

impl<T> future::Executor<T> for TaskExecutor
where T: Future<Item = (), Error = ()> + Send + 'static,
{
    fn execute(&self, future: T) -> Result<(), future::ExecuteError<T>> {
        self.inner.execute(future)
    }
}

impl ::executor::Executor for TaskExecutor {
    fn spawn(&mut self, future: Box<Future<Item = (), Error = ()> + Send>)
        -> Result<(), ::executor::SpawnError>
    {
        self.inner.spawn(future)
    }
}

// ===== impl Shutdown =====

impl Future for Shutdown {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        try_ready!(self.inner.poll());
        Ok(().into())
    }
}

impl fmt::Debug for Shutdown {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Shutdown")
            .field("inner", &"Box<Future<Item = (), Error = ()>>")
            .finish()
    }
}
