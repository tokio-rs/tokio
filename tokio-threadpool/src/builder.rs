use callback::Callback;
use config::{Config, MAX_WORKERS};
use park::{BoxPark, BoxedPark, DefaultPark};
use sender::Sender;
use pool::Inner;
use thread_pool::ThreadPool;
use worker::{self, Worker, WorkerId};

use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use num_cpus;
use tokio_executor::Enter;
use tokio_executor::park::Park;

#[cfg(feature = "unstable-futures")]
use futures2;

/// Builds a thread pool with custom configuration values.
///
/// Methods can be chanined in order to set the configuration values. The thread
/// pool is constructed by calling [`build`].
///
/// New instances of `Builder` are obtained via [`Builder::new`].
///
/// See function level documentation for details on the various configuration
/// settings.
///
/// [`build`]: #method.build
/// [`Builder::new`]: #method.new
///
/// # Examples
///
/// ```
/// # extern crate tokio_threadpool;
/// # extern crate futures;
/// # use tokio_threadpool::Builder;
/// use futures::future::{Future, lazy};
/// use std::time::Duration;
///
/// # pub fn main() {
/// // Create a thread pool with default configuration values
/// let thread_pool = Builder::new()
///     .pool_size(4)
///     .keep_alive(Some(Duration::from_secs(30)))
///     .build();
///
/// thread_pool.spawn(lazy(|| {
///     println!("called from a worker thread");
///     Ok(())
/// }));
///
/// // Gracefully shutdown the threadpool
/// thread_pool.shutdown().wait().unwrap();
/// # }
/// ```
pub struct Builder {
    /// Thread pool specific configuration values
    config: Config,

    /// Number of workers to spawn
    pool_size: usize,

    /// Generates the `Park` instances
    new_park: Box<Fn(&WorkerId) -> BoxPark>,
}

impl Builder {
    /// Returns a new thread pool builder initialized with default configuration
    /// values.
    ///
    /// Configuration methods can be chained on the return value.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    /// use std::time::Duration;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .pool_size(4)
    ///     .keep_alive(Some(Duration::from_secs(30)))
    ///     .build();
    /// # }
    /// ```
    pub fn new() -> Builder {
        let num_cpus = num_cpus::get();

        let new_park = Box::new(|_: &WorkerId| {
            Box::new(BoxedPark::new(DefaultPark::new()))
                as BoxPark
        });

        Builder {
            pool_size: num_cpus,
            config: Config {
                keep_alive: None,
                name_prefix: None,
                stack_size: None,
                around_worker: None,
            },
            new_park,
        }
    }

    /// Set the maximum number of worker threads for the thread pool instance.
    ///
    /// This must be a number between 1 and 32,768 though it is advised to keep
    /// this value on the smaller side.
    ///
    /// The default value is the number of cores available to the system.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .pool_size(4)
    ///     .build();
    /// # }
    /// ```
    pub fn pool_size(&mut self, val: usize) -> &mut Self {
        assert!(val >= 1, "at least one thread required");
        assert!(val <= MAX_WORKERS, "max value is {}", 32768);

        self.pool_size = val;
        self
    }

    /// Set the worker thread keep alive duration
    ///
    /// If set, a worker thread will wait for up to the specified duration for
    /// work, at which point the thread will shutdown. When work becomes
    /// available, a new thread will eventually be spawned to replace the one
    /// that shut down.
    ///
    /// When the value is `None`, the thread will wait for work forever.
    ///
    /// The default value is `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    /// use std::time::Duration;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .keep_alive(Some(Duration::from_secs(30)))
    ///     .build();
    /// # }
    /// ```
    pub fn keep_alive(&mut self, val: Option<Duration>) -> &mut Self {
        self.config.keep_alive = val;
        self
    }

    /// Set name prefix of threads spawned by the scheduler
    ///
    /// Thread name prefix is used for generating thread names. For example, if
    /// prefix is `my-pool-`, then threads in the pool will get names like
    /// `my-pool-1` etc.
    ///
    /// If this configuration is not set, then the thread will use the system
    /// default naming scheme.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .name_prefix("my-pool-")
    ///     .build();
    /// # }
    /// ```
    pub fn name_prefix<S: Into<String>>(&mut self, val: S) -> &mut Self {
        self.config.name_prefix = Some(val.into());
        self
    }

    /// Set the stack size (in bytes) for worker threads.
    ///
    /// The actual stack size may be greater than this value if the platform
    /// specifies minimal stack size.
    ///
    /// The default stack size for spawned threads is 2 MiB, though this
    /// particular stack size is subject to change in the future.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .stack_size(32 * 1024)
    ///     .build();
    /// # }
    /// ```
    pub fn stack_size(&mut self, val: usize) -> &mut Self {
        self.config.stack_size = Some(val);
        self
    }

    /// Execute function `f` on each worker thread.
    ///
    /// This function is provided a handle to the worker and is expected to call
    /// `Worker::run`, otherwise the worker thread will shutdown without doing
    /// any work.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .around_worker(|worker, _| {
    ///         println!("worker is starting up");
    ///         worker.run();
    ///         println!("worker is shutting down");
    ///     })
    ///     .build();
    /// # }
    /// ```
    pub fn around_worker<F>(&mut self, f: F) -> &mut Self
        where F: Fn(&Worker, &mut Enter) + Send + Sync + 'static
    {
        self.config.around_worker = Some(Callback::new(f));
        self
    }

    /// Customize the `park` instance used by each worker thread.
    ///
    /// The provided closure `f` is called once per worker and returns a `Park`
    /// instance that is used by the worker to put itself to sleep.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    /// # fn decorate<F>(f: F) -> F { f }
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .custom_park(|_| {
    ///         use tokio_threadpool::park::DefaultPark;
    ///
    ///         // This is the default park type that the worker would use if we
    ///         // did not customize it.
    ///         let park = DefaultPark::new();
    ///
    ///         // Decorate the `park` instance, allowing us to customize work
    ///         // that happens when a worker therad goes to sleep.
    ///         decorate(park)
    ///     })
    ///     .build();
    /// # }
    /// ```
    pub fn custom_park<F, P>(&mut self, f: F) -> &mut Self
    where F: Fn(&WorkerId) -> P + 'static,
          P: Park + Send + 'static,
          P::Error: Error,
    {
        self.new_park = Box::new(move |id| {
            Box::new(BoxedPark::new(f(id)))
        });

        self
    }

    /// Create the configured `ThreadPool`.
    ///
    /// The returned `ThreadPool` instance is ready to spawn tasks.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .build();
    /// # }
    /// ```
    pub fn build(&self) -> ThreadPool {
        let mut workers = vec![];

        trace!("build; num-workers={}", self.pool_size);

        for i in 0..self.pool_size {
            let id = WorkerId::new(i);
            let park = (self.new_park)(&id);
            let unpark = park.unpark();

            workers.push(worker::Entry::new(park, unpark));
        }

        // Create the pool
        let inner = Arc::new(
            Inner::new(
                workers.into_boxed_slice(),
                self.config.clone()));

        // Wrap with `Sender`
        let inner = Some(Sender {
            inner
        });

        ThreadPool { inner }
    }
}

impl fmt::Debug for Builder {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Builder")
            .field("config", &self.config)
            .field("pool_size", &self.pool_size)
            .field("new_park", &"Box<Fn() -> BoxPark>")
            .finish()
    }
}
