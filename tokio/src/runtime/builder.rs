use crate::runtime::handle::Handle;
use crate::runtime::{blocking, driver, Callback, Runtime, Spawner};

use std::fmt;
use std::io;
use std::time::Duration;

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
/// [`build`]: method@Self::build
/// [`Builder::new`]: method@Self::new
///
/// # Examples
///
/// ```
/// use tokio::runtime::Builder;
///
/// fn main() {
///     // build runtime
///     let runtime = Builder::new_multi_thread()
///         .worker_threads(4)
///         .thread_name("my-custom-name")
///         .thread_stack_size(3 * 1024 * 1024)
///         .build()
///         .unwrap();
///
///     // use runtime ...
/// }
/// ```
pub struct Builder {
    /// Runtime type
    kind: Kind,

    /// Whether or not to enable the I/O driver
    enable_io: bool,

    /// Whether or not to enable the time driver
    enable_time: bool,

    /// The number of worker threads, used by Runtime.
    ///
    /// Only used when not using the current-thread executor.
    worker_threads: Option<usize>,

    /// Cap on thread usage.
    max_threads: usize,

    /// Name fn used for threads spawned by the runtime.
    pub(super) thread_name: ThreadNameFn,

    /// Stack size used for threads spawned by the runtime.
    pub(super) thread_stack_size: Option<usize>,

    /// Callback to run after each thread starts.
    pub(super) after_start: Option<Callback>,

    /// To run before each worker thread stops
    pub(super) before_stop: Option<Callback>,

    /// Customizable keep alive timeout for BlockingPool
    pub(super) keep_alive: Option<Duration>,
}

pub(crate) type ThreadNameFn = std::sync::Arc<dyn Fn() -> String + Send + Sync + 'static>;

pub(crate) enum Kind {
    CurrentThread,
    #[cfg(feature = "rt-multi-thread")]
    MultiThread,
}

impl Builder {
    /// TODO
    pub fn new_current_thread() -> Builder {
        Builder::new(Kind::CurrentThread)
    }

    /// TODO
    #[cfg(feature = "rt-multi-thread")]
    pub fn new_multi_thread() -> Builder {
        Builder::new(Kind::MultiThread)
    }

    /// Returns a new runtime builder initialized with default configuration
    /// values.
    ///
    /// Configuration methods can be chained on the return value.
    pub(crate) fn new(kind: Kind) -> Builder {
        Builder {
            kind,

            // I/O defaults to "off"
            enable_io: false,

            // Time defaults to "off"
            enable_time: false,

            // Default to lazy auto-detection (one thread per CPU core)
            worker_threads: None,

            max_threads: 512,

            // Default thread name
            thread_name: std::sync::Arc::new(|| "tokio-runtime-worker".into()),

            // Do not set a stack size by default
            thread_stack_size: None,

            // No worker thread callbacks
            after_start: None,
            before_stop: None,

            keep_alive: None,
        }
    }

    /// Enables both I/O and time drivers.
    ///
    /// Doing this is a shorthand for calling `enable_io` and `enable_time`
    /// individually. If additional components are added to Tokio in the future,
    /// `enable_all` will include these future components.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime;
    ///
    /// let rt = runtime::Builder::new_multi_thread()
    ///     .enable_all()
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn enable_all(&mut self) -> &mut Self {
        #[cfg(any(feature = "net", feature = "process", all(unix, feature = "signal")))]
        self.enable_io();
        #[cfg(feature = "time")]
        self.enable_time();

        self
    }

    /// Sets the number of worker threads the `Runtime` will use.
    ///
    /// This should be a number between 0 and 32,768 though it is advised to
    /// keep this value on the smaller side.
    ///
    /// # Default
    ///
    /// The default value is the number of cores available to the system.
    ///
    /// # Panic
    ///
    /// When using the `current_thread` runtime this method will panic, since
    /// those variants do not allow setting worker thread counts.
    ///
    ///
    /// # Examples
    ///
    /// ## Multi threaded runtime with 4 threads
    ///
    /// ```
    /// use tokio::runtime;
    ///
    /// // This will spawn a work-stealing runtime with 4 worker threads.
    /// let rt = runtime::Builder::new_multi_thread()
    ///     .worker_threads(4)
    ///     .build()
    ///     .unwrap();
    ///
    /// rt.spawn(async move {});
    /// ```
    ///
    /// ## Current thread runtime (will only run on the current thread via `Runtime::block_on`)
    ///
    /// ```
    /// use tokio::runtime;
    ///
    /// // Create a runtime that _must_ be driven from a call
    /// // to `Runtime::block_on`.
    /// let rt = runtime::Builder::new_current_thread()
    ///     .build()
    ///     .unwrap();
    ///
    /// // This will run the runtime and future on the current thread
    /// rt.block_on(async move {});
    /// ```
    ///
    /// # Panic
    ///
    /// This will panic if `val` is not larger than `0`.
    pub fn worker_threads(&mut self, val: usize) -> &mut Self {
        assert!(val > 0, "Worker threads cannot be set to 0");
        self.worker_threads = Some(val);
        self
    }

    /// Specifies limit for threads, spawned by the Runtime.
    ///
    /// This is number of threads to be used by Runtime, including `core_threads`
    /// Having `max_threads` less than `worker_threads` results in invalid configuration
    /// when building multi-threaded `Runtime`, which would cause a panic.
    ///
    /// Similarly to the `worker_threads`, this number should be between 0 and 32,768.
    ///
    /// The default value is 512.
    ///
    /// When multi-threaded runtime is not used, will act as limit on additional threads.
    ///
    /// Otherwise as `core_threads` are always active, it limits additional threads (e.g. for
    /// blocking annotations) as `max_threads - core_threads`.
    pub fn max_threads(&mut self, val: usize) -> &mut Self {
        self.max_threads = val;
        self
    }

    /// Sets name of threads spawned by the `Runtime`'s thread pool.
    ///
    /// The default name is "tokio-runtime-worker".
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new_multi_thread()
    ///     .thread_name("my-pool")
    ///     .build();
    /// # }
    /// ```
    pub fn thread_name(&mut self, val: impl Into<String>) -> &mut Self {
        let val = val.into();
        self.thread_name = std::sync::Arc::new(move || val.clone());
        self
    }

    /// Sets a function used to generate the name of threads spawned by the `Runtime`'s thread pool.
    ///
    /// The default name fn is `|| "tokio-runtime-worker".into()`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio::runtime;
    /// # use std::sync::atomic::{AtomicUsize, Ordering};
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new_multi_thread()
    ///     .thread_name_fn(|| {
    ///        static ATOMIC_ID: AtomicUsize = AtomicUsize::new(0);
    ///        let id = ATOMIC_ID.fetch_add(1, Ordering::SeqCst);
    ///        format!("my-pool-{}", id)
    ///     })
    ///     .build();
    /// # }
    /// ```
    pub fn thread_name_fn<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        self.thread_name = std::sync::Arc::new(f);
        self
    }

    /// Sets the stack size (in bytes) for worker threads.
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
    /// # use tokio::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new_multi_thread()
    ///     .thread_stack_size(32 * 1024)
    ///     .build();
    /// # }
    /// ```
    pub fn thread_stack_size(&mut self, val: usize) -> &mut Self {
        self.thread_stack_size = Some(val);
        self
    }

    /// Executes function `f` after each thread is started but before it starts
    /// doing work.
    ///
    /// This is intended for bookkeeping and monitoring use cases.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio::runtime;
    ///
    /// # pub fn main() {
    /// let runtime = runtime::Builder::new_multi_thread()
    ///     .on_thread_start(|| {
    ///         println!("thread started");
    ///     })
    ///     .build();
    /// # }
    /// ```
    #[cfg(not(loom))]
    pub fn on_thread_start<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.after_start = Some(std::sync::Arc::new(f));
        self
    }

    /// Executes function `f` before each thread stops.
    ///
    /// This is intended for bookkeeping and monitoring use cases.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio::runtime;
    ///
    /// # pub fn main() {
    /// let runtime = runtime::Builder::new_multi_thread()
    ///     .on_thread_stop(|| {
    ///         println!("thread stopping");
    ///     })
    ///     .build();
    /// # }
    /// ```
    #[cfg(not(loom))]
    pub fn on_thread_stop<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.before_stop = Some(std::sync::Arc::new(f));
        self
    }

    /// Creates the configured `Runtime`.
    ///
    /// The returned `Runtime` instance is ready to spawn tasks.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime::Builder;
    ///
    /// let rt  = Builder::new_multi_thread().build().unwrap();
    ///
    /// rt.block_on(async {
    ///     println!("Hello from the Tokio runtime");
    /// });
    /// ```
    pub fn build(&mut self) -> io::Result<Runtime> {
        match &self.kind {
            Kind::CurrentThread => self.build_basic_runtime(),
            #[cfg(feature = "rt-multi-thread")]
            Kind::MultiThread => self.build_threaded_runtime(),
        }
    }

    fn get_cfg(&self) -> driver::Cfg {
        driver::Cfg {
            enable_io: self.enable_io,
            enable_time: self.enable_time,
        }
    }

    /// Sets a custom timeout for a thread in the blocking pool.
    ///
    /// By default, the timeout for a thread is set to 10 seconds. This can
    /// be overriden using .thread_keep_alive().
    ///
    /// # Example
    ///
    /// ```
    /// # use tokio::runtime;
    /// # use std::time::Duration;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new_multi_thread()
    ///     .thread_keep_alive(Duration::from_millis(100))
    ///     .build();
    /// # }
    /// ```
    pub fn thread_keep_alive(&mut self, duration: Duration) -> &mut Self {
        self.keep_alive = Some(duration);
        self
    }

    fn build_basic_runtime(&mut self) -> io::Result<Runtime> {
        use crate::runtime::{BasicScheduler, Kind};

        let (driver, resources) = driver::Driver::new(self.get_cfg())?;

        // And now put a single-threaded scheduler on top of the timer. When
        // there are no futures ready to do something, it'll let the timer or
        // the reactor to generate some new stimuli for the futures to continue
        // in their life.
        let scheduler = BasicScheduler::new(driver);
        let spawner = Spawner::Basic(scheduler.spawner().clone());

        // Blocking pool
        let blocking_pool = blocking::create_blocking_pool(self, self.max_threads);
        let blocking_spawner = blocking_pool.spawner().clone();

        Ok(Runtime {
            kind: Kind::CurrentThread(scheduler),
            handle: Handle {
                spawner,
                io_handle: resources.io_handle,
                time_handle: resources.time_handle,
                signal_handle: resources.signal_handle,
                clock: resources.clock,
                blocking_spawner,
            },
            blocking_pool,
        })
    }
}

cfg_io_driver! {
    impl Builder {
        /// Enables the I/O driver.
        ///
        /// Doing this enables using net, process, signal, and some I/O types on
        /// the runtime.
        ///
        /// # Examples
        ///
        /// ```
        /// use tokio::runtime;
        ///
        /// let rt = runtime::Builder::new_multi_thread()
        ///     .enable_io()
        ///     .build()
        ///     .unwrap();
        /// ```
        pub fn enable_io(&mut self) -> &mut Self {
            self.enable_io = true;
            self
        }
    }
}

cfg_time! {
    impl Builder {
        /// Enables the time driver.
        ///
        /// Doing this enables using `tokio::time` on the runtime.
        ///
        /// # Examples
        ///
        /// ```
        /// use tokio::runtime;
        ///
        /// let rt = runtime::Builder::new_multi_thread()
        ///     .enable_time()
        ///     .build()
        ///     .unwrap();
        /// ```
        pub fn enable_time(&mut self) -> &mut Self {
            self.enable_time = true;
            self
        }
    }
}

cfg_rt_multi_thread! {
    impl Builder {
        fn build_threaded_runtime(&mut self) -> io::Result<Runtime> {
            use crate::loom::sys::num_cpus;
            use crate::runtime::{Kind, ThreadPool};
            use crate::runtime::park::Parker;
            use std::cmp;

            let core_threads = self.worker_threads.unwrap_or_else(|| cmp::min(self.max_threads, num_cpus()));
            assert!(core_threads <= self.max_threads, "Core threads number cannot be above max limit");

            let (driver, resources) = driver::Driver::new(self.get_cfg())?;

            let (scheduler, launch) = ThreadPool::new(core_threads, Parker::new(driver));
            let spawner = Spawner::ThreadPool(scheduler.spawner().clone());

            // Create the blocking pool
            let blocking_pool = blocking::create_blocking_pool(self, self.max_threads);
            let blocking_spawner = blocking_pool.spawner().clone();

            // Create the runtime handle
            let handle = Handle {
                spawner,
                io_handle: resources.io_handle,
                time_handle: resources.time_handle,
                signal_handle: resources.signal_handle,
                clock: resources.clock,
                blocking_spawner,
            };

            // Spawn the thread pool workers
            let _enter = crate::runtime::context::enter(handle.clone());
            launch.launch();

            Ok(Runtime {
                kind: Kind::ThreadPool(scheduler),
                handle,
                blocking_pool,
            })
        }
    }
}

impl fmt::Debug for Builder {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Builder")
            .field("worker_threads", &self.worker_threads)
            .field("max_threads", &self.max_threads)
            .field(
                "thread_name",
                &"<dyn Fn() -> String + Send + Sync + 'static>",
            )
            .field("thread_stack_size", &self.thread_stack_size)
            .field("after_start", &self.after_start.as_ref().map(|_| "..."))
            .field("before_stop", &self.after_start.as_ref().map(|_| "..."))
            .finish()
    }
}
