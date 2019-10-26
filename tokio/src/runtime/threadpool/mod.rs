mod builder;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::builder::Builder;

mod spawner;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::spawner::Spawner;

#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use tokio_executor::thread_pool::JoinHandle;

use crate::net::driver;
use crate::timer::timer;

use tokio_executor::thread_pool::ThreadPool;

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
    pool: ThreadPool,

    /// Reactor handles
    reactor_handles: Vec<crate::net::driver::Handle>,

    /// Timer handles
    timer_handles: Vec<timer::Handle>,
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
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// // Use the runtime...
    /// ```
    ///
    /// [mod]: index.html
    pub fn new() -> io::Result<Self> {
        Builder::new().build()
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
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn<F>(&self, future: F) -> &Self
    where
        F: Future<Output = ()> + Send + 'static,
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
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        let _reactor = driver::set_default(&self.inner().reactor_handles[0]);
        let _timer = timer::set_default(&self.inner().timer_handles[0]);

        self.inner().pool.block_on(future)
    }

    /// Return a handle to the runtime's spawner.
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
    /// let spawner = rt.spawner();
    ///
    /// spawner.spawn(async { println!("hello"); });
    /// ```
    pub fn spawner(&self) -> Spawner {
        let inner = self.inner().pool.spawner().clone();
        Spawner::new(inner)
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
    #[allow(warnings)]
    pub fn shutdown_now(mut self) {
        self.inner.unwrap().pool.shutdown_now();
    }

    fn inner(&self) -> &Inner {
        self.inner.as_ref().unwrap()
    }
}
