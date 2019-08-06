mod background;
mod builder;
mod task_executor;

pub use self::builder::Builder;
pub use self::task_executor::TaskExecutor;
use background::Background;

use tokio_executor::enter;
use tokio_timer::timer;

use tracing_core as trace;
use std::future::Future;
use std::io;

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
    /// Task execution pool.
    pool: tokio_threadpool::ThreadPool,

    /// Tracing dispatcher
    trace: trace::Dispatch,

    /// Maintains a reactor and timer that are always running on a background
    /// thread. This is to support `runtime.block_on` w/o requiring the future
    /// to be `Send`.
    ///
    /// A dedicated background thread is required as the threadpool threads
    /// might not be running. However, this is a temporary work around.
    ///
    /// TODO: Delete this
    background: Background,
}

// ===== impl Runtime =====

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
    /// rt.shutdown_now();
    /// ```
    ///
    /// [mod]: index.html
    pub fn new() -> io::Result<Self> {
        Builder::new().build()
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
    /// ```
    /// #![feature(async_await)]
    ///
    /// use tokio::runtime::Runtime;
    ///
    /// fn main() {
    ///    // Create the runtime
    ///    let rt = Runtime::new().unwrap();
    ///
    ///    // Spawn a future onto the runtime
    ///    rt.spawn(async {
    ///        println!("now running on a worker thread");
    ///    });
    ///
    ///    rt.shutdown_on_idle();
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn<F>(&self, future: F) -> &Self
    where F: Future<Output = ()> + Send + 'static,
    {
        self.inner().pool.spawn(future);
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
    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        let mut entered = enter().expect("nested block_on");

        let bg = &self.inner().background;
        let trace = &self.inner().trace;

        tokio_executor::with_default(&mut self.inner().pool.sender(), || {
            tokio_reactor::with_default(bg.reactor(), || {
                timer::with_default(bg.timer(), || {
                    trace::dispatcher::with_default(trace, || {
                        entered.block_on(future)
                    })
                })
            })
        })
    }

    /// Signals the runtime to shutdown once it becomes idle.
    ///
    /// Blocks the current thread until the shutdown operation has completed.
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
    /// rt.shutdown_on_idle();
    /// ```
    ///
    /// [mod]: index.html
    pub fn shutdown_on_idle(mut self) {
        let mut e = tokio_executor::enter().unwrap();

        let inner = self.inner.take().unwrap();
        e.block_on(inner.pool.shutdown_on_idle());
    }

    /// Signals the runtime to shutdown immediately.
    ///
    /// Blocks the current thread until the shutdown operation has completed.
    /// This function will forcibly shutdown the runtime, causing any
    /// in-progress work to become canceled.
    ///
    /// The shutdown steps are:
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
    /// rt.shutdown_now();
    /// ```
    ///
    /// [mod]: index.html
    pub fn shutdown_now(mut self) {
        let mut e = tokio_executor::enter().unwrap();
        let inner = self.inner.take().unwrap();

        e.block_on(inner.pool.shutdown_now());
    }

    fn inner(&self) -> &Inner {
        self.inner.as_ref().unwrap()
    }
}
