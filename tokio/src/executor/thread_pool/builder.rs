use crate::executor::park::Park;
use crate::executor::thread_pool::{shutdown, worker, worker::Worker, Spawner, ThreadPool};
use crate::loom::sync::Arc;
use crate::loom::sys::num_cpus;

use std::{fmt, usize};

/// Builds a thread pool with custom configuration values.
pub(crate) struct Builder {
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
    pub(crate) fn new() -> Builder {
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
    pub(crate) fn num_threads(&mut self, value: usize) -> &mut Self {
        self.pool_size = value;
        self
    }

    /// Set name of threads spawned by the scheduler
    ///
    /// If this configuration is not set, then the thread will use the system
    /// default naming scheme.
    pub(crate) fn name<S: Into<String>>(&mut self, val: S) -> &mut Self {
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
    pub(crate) fn stack_size(&mut self, val: usize) -> &mut Self {
        self.stack_size = Some(val);
        self
    }

    /// Execute function `f` on each worker thread.
    ///
    /// This function is provided a function that executes the worker and is
    /// expected to call it, otherwise the worker thread will shutdown without
    /// doing any work.
    pub(crate) fn around_worker<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(usize, &mut dyn FnMut()) + Send + Sync + 'static,
    {
        self.around_worker = Some(Arc::new(Box::new(f)));
        self
    }

    /// Create the configured `ThreadPool` with a custom `park` instances.
    ///
    /// The provided closure `build_park` is called once per worker and returns
    /// a `Park` instance that is used by the worker to put itself to sleep.
    pub(crate) fn build<F, P>(&self, mut build_park: F) -> ThreadPool
    where
        F: FnMut(usize) -> P,
        P: Park + Send + 'static,
    {
        let (shutdown_tx, shutdown_rx) = shutdown::channel();

        let around_worker = self.around_worker.as_ref().map(Arc::clone);
        let launch_worker = Arc::new(Box::new(move |worker: Worker<BoxedPark<P>>| {
            // NOTE: It might seem like the shutdown_tx that's moved into this Arc is never
            // dropped, and that shutdown_rx will therefore never see EOF, but that is not actually
            // the case. Only `build_with_park` and each worker hold onto a copy of this Arc.
            // `build_with_park` drops it immediately, and the workers drop theirs when their `run`
            // method returns (and their copy of the Arc are dropped). In fact, we don't actually
            // _need_ a copy of `shutdown_tx` for each worker thread; having them all hold onto
            // this Arc, which in turn holds the last `shutdown_tx` would have been sufficient.
            let shutdown_tx = shutdown_tx.clone();
            let around_worker = around_worker.as_ref().map(Arc::clone);
            Box::new(move || {
                struct AbortOnPanic;

                impl Drop for AbortOnPanic {
                    fn drop(&mut self) {
                        if std::thread::panicking() {
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
        })
            as Box<dyn Fn(Worker<BoxedPark<P>>) -> Box<dyn FnOnce() + Send> + Send + Sync>);

        let mut blocking = crate::executor::blocking::Builder::default();
        blocking.name(self.name.clone());
        if let Some(ss) = self.stack_size {
            blocking.stack_size(ss);
        }
        let blocking = Arc::new(blocking.build());

        let (pool, workers) = worker::create_set::<_, BoxedPark<P>>(
            self.pool_size,
            |i| Box::new(BoxedPark::new(build_park(i))),
            Arc::clone(&launch_worker),
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
