use callback::Callback;
use config::{Config, MAX_WORKERS};
use park::{BoxPark, BoxedPark, DefaultPark};
use pool::{Pool, MAX_BACKUP};
use shutdown::ShutdownTrigger;
use thread_pool::ThreadPool;
use worker::{self, Worker, WorkerId};

use std::any::Any;
use std::cmp::max;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_deque::Injector;
use num_cpus;
use tokio_executor::park::Park;
use tokio_executor::Enter;

/// Builds a thread pool with custom configuration values.
///
/// Methods can be chained in order to set the configuration values. The thread
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

    /// Maximum number of futures that can be in a blocking section
    /// concurrently.
    max_blocking: usize,

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
    /// let thread_pool = Builder::new()
    ///     .pool_size(4)
    ///     .keep_alive(Some(Duration::from_secs(30)))
    ///     .build();
    /// # }
    /// ```
    pub fn new() -> Builder {
        let num_cpus = max(1, num_cpus::get());

        let new_park =
            Box::new(|_: &WorkerId| Box::new(BoxedPark::new(DefaultPark::new())) as BoxPark);

        Builder {
            pool_size: num_cpus,
            max_blocking: 100,
            config: Config {
                keep_alive: None,
                name_prefix: None,
                stack_size: None,
                around_worker: None,
                after_start: None,
                before_stop: None,
                panic_handler: None,
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
    /// let thread_pool = Builder::new()
    ///     .pool_size(4)
    ///     .build();
    /// # }
    /// ```
    pub fn pool_size(&mut self, val: usize) -> &mut Self {
        assert!(val >= 1, "at least one thread required");
        assert!(val <= MAX_WORKERS, "max value is {}", MAX_WORKERS);

        self.pool_size = val;
        self
    }

    /// Set the maximum number of concurrent blocking sections.
    ///
    /// When the maximum concurrent `blocking` calls is reached, any further
    /// calls to `blocking` will return `NotReady` and the task is notified once
    /// previously in-flight calls to `blocking` return.
    ///
    /// This must be a number between 1 and 32,768 though it is advised to keep
    /// this value on the smaller side.
    ///
    /// The default value is 100.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// let thread_pool = Builder::new()
    ///     .max_blocking(200)
    ///     .build();
    /// # }
    /// ```
    pub fn max_blocking(&mut self, val: usize) -> &mut Self {
        assert!(val <= MAX_BACKUP, "max value is {}", MAX_BACKUP);
        self.max_blocking = val;
        self
    }

    /// Set the thread keep alive duration
    ///
    /// If set, a thread that has completed a `blocking` call will wait for up
    /// to the specified duration to become a worker thread again. Once the
    /// duration elapses, the thread will shutdown.
    ///
    /// When the value is `None`, the thread will wait to become a worker
    /// thread forever.
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
    /// let thread_pool = Builder::new()
    ///     .keep_alive(Some(Duration::from_secs(30)))
    ///     .build();
    /// # }
    /// ```
    pub fn keep_alive(&mut self, val: Option<Duration>) -> &mut Self {
        self.config.keep_alive = val;
        self
    }

    /// Sets a callback to be triggered when a panic during a future bubbles up
    /// to Tokio. By default Tokio catches these panics, and they will be
    /// ignored. The parameter passed to this callback is the same error value
    /// returned from std::panic::catch_unwind(). To abort the process on
    /// panics, use std::panic::resume_unwind() in this callback as shown
    /// below.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// let thread_pool = Builder::new()
    ///     .panic_handler(|err| std::panic::resume_unwind(err))
    ///     .build();
    /// # }
    /// ```
    pub fn panic_handler<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(Box<Any + Send>) + Send + Sync + 'static,
    {
        self.config.panic_handler = Some(Arc::new(f));
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
    /// [`Worker::run`], otherwise the worker thread will shutdown without doing
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
    /// let thread_pool = Builder::new()
    ///     .around_worker(|worker, _| {
    ///         println!("worker is starting up");
    ///         worker.run();
    ///         println!("worker is shutting down");
    ///     })
    ///     .build();
    /// # }
    /// ```
    ///
    /// [`Worker::run`]: struct.Worker.html#method.run
    pub fn around_worker<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(&Worker, &mut Enter) + Send + Sync + 'static,
    {
        self.config.around_worker = Some(Callback::new(f));
        self
    }

    /// Execute function `f` after each thread is started but before it starts
    /// doing work.
    ///
    /// This is intended for bookkeeping and monitoring use cases.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// let thread_pool = Builder::new()
    ///     .after_start(|| {
    ///         println!("thread started");
    ///     })
    ///     .build();
    /// # }
    /// ```
    pub fn after_start<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.config.after_start = Some(Arc::new(f));
        self
    }

    /// Execute function `f` before each thread stops.
    ///
    /// This is intended for bookkeeping and monitoring use cases.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// let thread_pool = Builder::new()
    ///     .before_stop(|| {
    ///         println!("thread stopping");
    ///     })
    ///     .build();
    /// # }
    /// ```
    pub fn before_stop<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.config.before_stop = Some(Arc::new(f));
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
    /// let thread_pool = Builder::new()
    ///     .custom_park(|_| {
    ///         use tokio_threadpool::park::DefaultPark;
    ///
    ///         // This is the default park type that the worker would use if we
    ///         // did not customize it.
    ///         let park = DefaultPark::new();
    ///
    ///         // Decorate the `park` instance, allowing us to customize work
    ///         // that happens when a worker thread goes to sleep.
    ///         decorate(park)
    ///     })
    ///     .build();
    /// # }
    /// ```
    pub fn custom_park<F, P>(&mut self, f: F) -> &mut Self
    where
        F: Fn(&WorkerId) -> P + 'static,
        P: Park + Send + 'static,
        P::Error: Error,
    {
        self.new_park = Box::new(move |id| Box::new(BoxedPark::new(f(id))));

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
    /// let thread_pool = Builder::new()
    ///     .build();
    /// # }
    /// ```
    pub fn build(&self) -> ThreadPool {
        trace!("build; num-workers={}", self.pool_size);

        // Create the worker entry list
        let workers: Arc<[worker::Entry]> = {
            let mut workers = vec![];

            for i in 0..self.pool_size {
                let id = WorkerId::new(i);
                let park = (self.new_park)(&id);
                let unpark = park.unpark();

                workers.push(worker::Entry::new(park, unpark));
            }

            workers.into()
        };

        let queue = Arc::new(Injector::new());

        // Create a trigger that will clean up resources on shutdown.
        //
        // The `Pool` contains a weak reference to it, while `Worker`s and the `ThreadPool` contain
        // strong references.
        let trigger = Arc::new(ShutdownTrigger::new(workers.clone(), queue.clone()));

        // Create the pool
        let pool = Arc::new(Pool::new(
            workers,
            Arc::downgrade(&trigger),
            self.max_blocking,
            self.config.clone(),
            queue,
        ));

        ThreadPool::new2(pool, trigger)
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
