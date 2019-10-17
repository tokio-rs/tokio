use crate::loom::sync::Arc;
use crate::loom::sys::num_cpus;
use crate::loom::thread;
use crate::park::Park;
use crate::thread_pool::park::DefaultPark;
use crate::thread_pool::{shutdown, worker, Spawner, ThreadPool};

use std::{fmt, usize};

/// Builds a thread pool with custom configuration values.
pub struct Builder {
    /// Number of threads to spawn
    pool_size: usize,

    /// Thread name prefix
    name_prefix: String,

    /// Thread stack size
    stack_size: Option<usize>,

    /// Around worker callback
    around_worker: Option<Arc<Callback>>,
}

type Callback = Box<dyn Fn(usize, &mut dyn FnMut()) + Send + Sync>;

impl Builder {
    /// Returns a new thread pool builder initialized with default configuration
    /// values.
    pub fn new() -> Builder {
        Builder {
            pool_size: num_cpus(),
            name_prefix: "tokio-runtime-worker-".to_string(),
            stack_size: None,
            around_worker: None,
        }
    }

    /// Set the number of threads running async tasks.
    ///
    /// This must be a number between 1 and 2,048 though it is advised to keep
    /// this value on the smaller side.
    ///
    /// The default value is the number of cores available to the system.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_executor::thread_pool::Builder;
    ///
    /// let thread_pool = Builder::new()
    ///     .num_threads(4)
    ///     .build();
    /// ```
    pub fn num_threads(&mut self, value: usize) -> &mut Self {
        self.pool_size = value;
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
    /// use tokio_executor::thread_pool::Builder;
    ///
    /// let thread_pool = Builder::new()
    ///     .name_prefix("my-pool-")
    ///     .build();
    /// ```
    pub fn name_prefix<S: Into<String>>(&mut self, val: S) -> &mut Self {
        self.name_prefix = val.into();
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
    /// use tokio_executor::thread_pool::Builder;
    ///
    /// let thread_pool = Builder::new()
    ///     .stack_size(32 * 1024)
    ///     .build();
    /// ```
    pub fn stack_size(&mut self, val: usize) -> &mut Self {
        self.stack_size = Some(val);
        self
    }

    /// Execute function `f` on each worker thread.
    ///
    /// This function is provided a function that executes the worker and is
    /// expected to call it, otherwise the worker thread will shutdown without
    /// doing any work.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_executor::thread_pool::Builder;
    ///
    /// let thread_pool = Builder::new()
    ///     .around_worker(|index, work| {
    ///         println!("worker {} is starting up", index);
    ///         work();
    ///         println!("worker {} is shutting down", index);
    ///     })
    ///     .build();
    /// ```
    pub fn around_worker<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(usize, &mut dyn FnMut()) + Send + Sync + 'static,
    {
        self.around_worker = Some(Arc::new(Box::new(f)));
        self
    }

    /// Create the configured `ThreadPool`.
    ///
    /// The returned `ThreadPool` instance is ready to spawn tasks.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_executor::thread_pool::Builder;
    ///
    /// let thread_pool = Builder::new()
    ///     .build();
    /// ```
    pub fn build(&self) -> ThreadPool {
        self.build_with_park(|_| DefaultPark::new())
    }

    /// Create the configured `ThreadPool` with a custom `park` instances.
    ///
    /// The provided closure `build_park` is called once per worker and returns
    /// a `Park` instance that is used by the worker to put itself to sleep.
    pub fn build_with_park<F, P>(&self, mut build_park: F) -> ThreadPool
    where
        F: FnMut(usize) -> P,
        P: Park + Send + 'static,
    {
        let (shutdown_tx, shutdown_rx) = shutdown::channel();

        let (pool, workers) = worker::create_set(self.pool_size, |i| BoxedPark::new(build_park(i)));

        // Spawn threads for each worker
        for (idx, mut worker) in workers.into_iter().enumerate() {
            let around_worker = self.around_worker.clone();
            let shutdown_tx = shutdown_tx.clone();

            let mut th = thread::Builder::new().name(format!("{}{}", self.name_prefix, idx));

            if let Some(stack) = self.stack_size {
                th = th.stack_size(stack);
            }

            let res = th.spawn(move || {
                struct AbortOnPanic;

                impl Drop for AbortOnPanic {
                    fn drop(&mut self) {
                        if thread::panicking() {
                            eprintln!("[ERROR] unhandled panic in Tokio scheduler. This is a bug and should be reported.");
                            std::process::abort();
                        }
                    }
                }

                let _abort_on_panic = AbortOnPanic;

                if let Some(cb) = around_worker {
                    cb(idx, &mut || worker.run());
                } else {
                    worker.run();
                }

                // Worker must be dropped before the `shutdown_tx`
                drop(worker);

                // Dropping the handle must happen __after__ the callback
                drop(shutdown_tx);
            });

            if let Err(err) = res {
                panic!("failed to spawn worker thread: {:?}", err);
            }
        }

        let spawner = Spawner::new(pool);
        ThreadPool::from_parts(spawner, shutdown_rx)
    }
}

impl Default for Builder {
    fn default() -> Builder {
        Builder::new()
    }
}

impl fmt::Debug for Builder {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Builder")
            .field("pool_size", &self.pool_size)
            .field("name_prefix", &self.name_prefix)
            .field("stack_size", &self.stack_size)
            .finish()
    }
}

pub(crate) struct BoxedPark<P> {
    inner: P,
}

impl<P> BoxedPark<P> {
    pub(crate) fn new(inner: P) -> Self {
        BoxedPark { inner }
    }
}

impl<P> Park for BoxedPark<P>
where
    P: Park,
{
    type Unpark = Box<dyn crate::park::Unpark>;
    type Error = P::Error;

    fn unpark(&self) -> Self::Unpark {
        Box::new(self.inner.unpark())
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.inner.park()
    }

    fn park_timeout(&mut self, duration: std::time::Duration) -> Result<(), Self::Error> {
        self.inner.park_timeout(duration)
    }
}
