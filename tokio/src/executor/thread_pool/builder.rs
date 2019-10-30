use crate::executor::loom::sync::Arc;
use crate::executor::loom::sys::num_cpus;
use crate::executor::loom::thread;
use crate::executor::park::Park;
use crate::executor::thread_pool::park::DefaultPark;
use crate::executor::thread_pool::{shutdown, worker, worker::Worker, Spawner, ThreadPool};

use std::{fmt, usize};

/// Builds a thread pool with custom configuration values.
pub struct Builder {
    /// Number of worker threads to spawn
    pool_size: usize,

    /// Thread name
    name: String,

    /// Thread stack size
    stack_size: Option<usize>,

    /// Around worker callback
    around_worker: Option<Callback>,
}

// The Arc<Box<_>> is needed because loom doesn't support Arc<T> where T: !Sized
// loom doesn't support that because it requires CoerceUnsized, which is unstable
type Callback = Arc<Box<dyn Fn(usize, &mut dyn FnMut()) + Send + Sync>>;

impl Builder {
    /// Returns a new thread pool builder initialized with default configuration
    /// values.
    pub fn new() -> Builder {
        Builder {
            pool_size: num_cpus(),
            name: "tokio-runtime-worker".to_string(),
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
    /// use tokio::executor::thread_pool::Builder;
    ///
    /// let thread_pool = Builder::new()
    ///     .num_threads(4)
    ///     .build();
    /// ```
    pub fn num_threads(&mut self, value: usize) -> &mut Self {
        self.pool_size = value;
        self
    }

    /// Set name of threads spawned by the scheduler
    ///
    /// If this configuration is not set, then the thread will use the system
    /// default naming scheme.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::executor::thread_pool::Builder;
    ///
    /// let thread_pool = Builder::new()
    ///     .name("my-pool")
    ///     .build();
    /// ```
    pub fn name<S: Into<String>>(&mut self, val: S) -> &mut Self {
        self.name = val.into();
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
    /// use tokio::executor::thread_pool::Builder;
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
    /// use tokio::executor::thread_pool::Builder;
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
    /// use tokio::executor::thread_pool::Builder;
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

        let around_worker = self.around_worker.as_ref().map(Arc::clone);
        let launch_worker = move |worker: Worker<BoxedPark<P>>| {
            let shutdown_tx = shutdown_tx.clone();
            let around_worker = around_worker.as_ref().map(Arc::clone);
            Box::new(move || {
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
                if let Some(cb) = around_worker.as_ref() {
                    let idx = worker.id();
                    let mut f = Some(move || worker.run());
                    cb(idx, &mut || {
                        (f.take()
                            .expect("around_thread callback called closure twice"))(
                        )
                    })
                } else {
                    worker.run()
                }

                // Dropping the handle must happen __after__ the callback
                drop(shutdown_tx);
            }) as Box<dyn FnOnce() + Send + 'static>
        };

        let mut blocking = crate::executor::blocking::Builder::default();
        blocking.name(self.name.clone());
        if let Some(ss) = self.stack_size {
            blocking.stack_size(ss);
        }
        let blocking = Arc::new(blocking.build());

        let (pool, workers) = worker::create_set::<_, BoxedPark<P>>(
            self.pool_size,
            |i| BoxedPark::new(build_park(i)),
            blocking.clone(),
        );

        // Spawn threads for each worker
        for worker in workers {
            crate::executor::blocking::Pool::spawn(&blocking, launch_worker(worker))
        }

        let spawner = Spawner::new(pool);
        let blocking = crate::executor::blocking::PoolWaiter::from(blocking);
        ThreadPool::from_parts(spawner, shutdown_rx, blocking)
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
            .field("name", &self.name)
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
    type Unpark = Box<dyn crate::executor::park::Unpark>;
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
