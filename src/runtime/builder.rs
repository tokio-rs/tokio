use runtime::{Inner, Runtime};

use reactor::Reactor;

use std::io;

use tokio_reactor;
use tokio_threadpool::Builder as ThreadPoolBuilder;
use tokio_threadpool::park::DefaultPark;
use tokio_timer::clock::{self, Clock};
use tokio_timer::timer::{self, Timer};

/// Builds Tokio Runtime with custom configuration values.
///
/// Methods can be chained in order to set the configuration values. The
/// Runtime is constructed by calling [`build`].
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
/// # extern crate tokio;
/// # extern crate tokio_threadpool;
/// # use tokio::runtime::Builder;
///
/// # pub fn main() {
/// // create and configure ThreadPool
/// let mut threadpool_builder = tokio_threadpool::Builder::new();
/// threadpool_builder
///     .name_prefix("my-runtime-worker-")
///     .pool_size(4);
///
/// // build Runtime
/// let runtime = Builder::new()
///     .threadpool_builder(threadpool_builder)
///     .build();
/// // ... call runtime.run(...)
/// # let _ = runtime;
/// # }
/// ```
#[derive(Debug)]
pub struct Builder {
    /// Thread pool specific builder
    threadpool_builder: ThreadPoolBuilder,

    /// The clock to use
    clock: Clock,
}

impl Builder {
    /// Returns a new runtime builder initialized with default configuration
    /// values.
    ///
    /// Configuration methods can be chained on the return value.
    pub fn new() -> Builder {
        let mut threadpool_builder = ThreadPoolBuilder::new();
        threadpool_builder.name_prefix("tokio-runtime-worker-");

        Builder {
            threadpool_builder,
            clock: Clock::new(),
        }
    }

    /// Set the `Clock` instance that will be used by the runtime.
    pub fn clock(&mut self, clock: Clock) -> &mut Self {
        self.clock = clock;
        self
    }

    /// Set builder to set up the thread pool instance.
    #[deprecated(
        since="0.1.9",
        note="use the `core_threads`, `blocking_threads`, `name_prefix`, \
              and `stack_size` functions on `runtime::Builder`, instead")]
    #[doc(hidden)]
    pub fn threadpool_builder(&mut self, val: ThreadPoolBuilder) -> &mut Self {
        self.threadpool_builder = val;
        self
    }

    /// Set the maximum number of worker threads for the `Runtime`'s thread pool.
    ///
    /// This must be a number between 1 and 32,768 though it is advised to keep
    /// this value on the smaller side.
    ///
    /// The default value is the number of cores available to the system.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # extern crate futures;
    /// # use tokio::runtime;
    ///
    /// # pub fn main() {
    /// let mut rt = runtime::Builder::new()
    ///     .core_threads(4)
    ///     .build()
    ///     .unwrap();
    /// # }
    /// ```
    pub fn core_threads(&mut self, val: usize) -> &mut Self {
        self.threadpool_builder.pool_size(val);
        self
    }

    /// Set the maximum number of concurrent blocking sections in the `Runtime`'s
    /// thread pool.
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
    /// # extern crate tokio;
    /// # extern crate futures;
    /// # use tokio::runtime;
    ///
    /// # pub fn main() {
    /// let mut rt = runtime::Builder::new()
    ///     .blocking_threads(200)
    ///     .build();
    /// # }
    /// ```
    pub fn blocking_threads(&mut self, val: usize) -> &mut Self {
        self.threadpool_builder.max_blocking(val);
        self
    }

    /// Set name prefix of threads spawned by the `Runtime`'s thread pool.
    ///
    /// Thread name prefix is used for generating thread names. For example, if
    /// prefix is `my-pool-`, then threads in the pool will get names like
    /// `my-pool-1` etc.
    ///
    /// The default prefix is "tokio-runtime-worker-".
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # extern crate futures;
    /// # use tokio::runtime;
    ///
    /// # pub fn main() {
    /// let mut rt = runtime::Builder::new()
    ///     .name_prefix("my-pool-")
    ///     .build();
    /// # }
    /// ```
    pub fn name_prefix<S: Into<String>>(&mut self, val: S) -> &mut Self {
        self.threadpool_builder.name_prefix(val);
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
    /// # extern crate tokio;
    /// # extern crate futures;
    /// # use tokio::runtime;
    ///
    /// # pub fn main() {
    /// let mut rt = runtime::Builder::new()
    ///     .stack_size(32 * 1024)
    ///     .build();
    /// # }
    /// ```
    pub fn stack_size(&mut self, val: usize) -> &mut Self {
        self.threadpool_builder.stack_size(val);
        self
    }

    /// Create the configured `Runtime`.
    ///
    /// The returned `ThreadPool` instance is ready to spawn tasks.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::runtime::Builder;
    /// # pub fn main() {
    /// let runtime = Builder::new().build().unwrap();
    /// // ... call runtime.run(...)
    /// # let _ = runtime;
    /// # }
    /// ```
    pub fn build(&mut self) -> io::Result<Runtime> {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        // Get a handle to the clock for the runtime.
        let clock1 = self.clock.clone();
        let clock2 = clock1.clone();

        let timers = Arc::new(Mutex::new(HashMap::<_, timer::Handle>::new()));
        let t1 = timers.clone();

        // Spawn a reactor on a background thread.
        let reactor = Reactor::new()?.background()?;

        // Get a handle to the reactor.
        let reactor_handle = reactor.handle().clone();

        let pool = self.threadpool_builder
            .around_worker(move |w, enter| {
                let timer_handle = t1.lock().unwrap()
                    .get(w.id()).unwrap()
                    .clone();

                tokio_reactor::with_default(&reactor_handle, enter, |enter| {
                    clock::with_default(&clock1, enter, |enter| {
                        timer::with_default(&timer_handle, enter, |_| {
                            w.run();
                        });
                    })
                });
            })
            .custom_park(move |worker_id| {
                // Create a new timer
                let timer = Timer::new_with_now(DefaultPark::new(), clock2.clone());

                timers.lock().unwrap()
                    .insert(worker_id.clone(), timer.handle());

                timer
            })
            .build();

        Ok(Runtime {
            inner: Some(Inner {
                reactor,
                pool,
            }),
        })
    }
}
