use crate::runtime::handle::Handle;
use crate::runtime::shell::Shell;
use crate::runtime::{blocking, io, time, Callback, Runtime, Spawner};

use std::fmt;
#[cfg(not(loom))]
use std::sync::Arc;

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
/// use tokio::runtime::Builder;
///
/// fn main() {
///     // build runtime
///     let runtime = Builder::new()
///         .threaded_scheduler()
///         .core_threads(4)
///         .thread_name("my-custom-name")
///         .thread_stack_size(3 * 1024 * 1024)
///         .build()
///         .unwrap();
///
///     // use runtime ...
/// }
/// ```
pub struct Builder {
    /// The task execution model to use.
    kind: Kind,

    /// Whether or not to enable the I/O driver
    enable_io: bool,

    /// Whether or not to enable the time driver
    enable_time: bool,

    /// The number of worker threads, used by Runtime.
    ///
    /// Only used when not using the current-thread executor.
    core_threads: Option<usize>,

    /// Cap on thread usage.
    max_threads: usize,

    /// Name used for threads spawned by the runtime.
    pub(super) thread_name: String,

    /// Stack size used for threads spawned by the runtime.
    pub(super) thread_stack_size: Option<usize>,

    /// Callback to run after each thread starts.
    pub(super) after_start: Option<Callback>,

    /// To run before each worker thread stops
    pub(super) before_stop: Option<Callback>,
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    Shell,
    #[cfg(feature = "rt-core")]
    Basic,
    #[cfg(feature = "rt-threaded")]
    ThreadPool,
}

impl Builder {
    /// Returns a new runtime builder initialized with default configuration
    /// values.
    ///
    /// Configuration methods can be chained on the return value.
    pub fn new() -> Builder {
        Builder {
            // No task execution by default
            kind: Kind::Shell,

            // I/O defaults to "off"
            enable_io: false,

            // Time defaults to "off"
            enable_time: false,

            // Default to lazy auto-detection (one thread per CPU core)
            core_threads: None,

            max_threads: 512,

            // Default thread name
            thread_name: "tokio-runtime-worker".into(),

            // Do not set a stack size by default
            thread_stack_size: None,

            // No worker thread callbacks
            after_start: None,
            before_stop: None,
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
    /// let rt = runtime::Builder::new()
    ///     .threaded_scheduler()
    ///     .enable_all()
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn enable_all(&mut self) -> &mut Self {
        #[cfg(feature = "io-driver")]
        self.enable_io();
        #[cfg(feature = "time")]
        self.enable_time();

        self
    }

    #[deprecated(note = "In future will be replaced by core_threads method")]
    /// Sets the maximum number of worker threads for the `Runtime`'s thread pool.
    ///
    /// This must be a number between 1 and 32,768 though it is advised to keep
    /// this value on the smaller side.
    ///
    /// The default value is the number of cores available to the system.
    pub fn num_threads(&mut self, val: usize) -> &mut Self {
        self.core_threads = Some(val);
        self
    }

    /// Sets the core number of worker threads for the `Runtime`'s thread pool.
    ///
    /// This should be a number between 1 and 32,768 though it is advised to keep
    /// this value on the smaller side.
    ///
    /// The default value is the number of cores available to the system.
    ///
    /// These threads will be always active and running.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime;
    ///
    /// let rt = runtime::Builder::new()
    ///     .threaded_scheduler()
    ///     .core_threads(4)
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn core_threads(&mut self, val: usize) -> &mut Self {
        assert_ne!(val, 0, "Core threads cannot be zero");
        self.core_threads = Some(val);
        self
    }

    /// Specifies limit for threads, spawned by the Runtime.
    ///
    /// This is number of threads to be used by Runtime, including `core_threads`
    /// Having `max_threads` less than `core_threads` results in invalid configuration
    /// when building multi-threaded `Runtime`, which would cause a panic.
    ///
    /// Similarly to the `core_threads`, this number should be between 1 and 32,768.
    ///
    /// The default value is 512.
    ///
    /// When multi-threaded runtime is not used, will act as limit on additional threads.
    ///
    /// Otherwise as `core_threads` are always active, it limits additional threads (e.g. for
    /// blocking annotations) as `max_threads - core_threads`.
    pub fn max_threads(&mut self, val: usize) -> &mut Self {
        assert_ne!(val, 0, "Thread limit cannot be zero");
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
    /// let rt = runtime::Builder::new()
    ///     .thread_name("my-pool")
    ///     .build();
    /// # }
    /// ```
    pub fn thread_name(&mut self, val: impl Into<String>) -> &mut Self {
        self.thread_name = val.into();
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
    /// let rt = runtime::Builder::new()
    ///     .threaded_scheduler()
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
    /// let runtime = runtime::Builder::new()
    ///     .threaded_scheduler()
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
        self.after_start = Some(Arc::new(f));
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
    /// let runtime = runtime::Builder::new()
    ///     .threaded_scheduler()
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
        self.before_stop = Some(Arc::new(f));
        self
    }

    /// Creates the configured `Runtime`.
    ///
    /// The returned `ThreadPool` instance is ready to spawn tasks.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime::Builder;
    ///
    /// let mut rt = Builder::new().build().unwrap();
    ///
    /// rt.block_on(async {
    ///     println!("Hello from the Tokio runtime");
    /// });
    /// ```
    pub fn build(&mut self) -> io::Result<Runtime> {
        match self.kind {
            Kind::Shell => self.build_shell_runtime(),
            #[cfg(feature = "rt-core")]
            Kind::Basic => self.build_basic_runtime(),
            #[cfg(feature = "rt-threaded")]
            Kind::ThreadPool => self.build_threaded_runtime(),
        }
    }

    fn build_shell_runtime(&mut self) -> io::Result<Runtime> {
        use crate::runtime::Kind;

        let clock = time::create_clock();

        // Create I/O driver
        let (io_driver, io_handle) = io::create_driver(self.enable_io)?;
        let (driver, time_handle) = time::create_driver(self.enable_time, io_driver, clock.clone());

        let spawner = Spawner::Shell;

        let blocking_pool = blocking::create_blocking_pool(self, self.max_threads);
        let blocking_spawner = blocking_pool.spawner().clone();

        Ok(Runtime {
            kind: Kind::Shell(Shell::new(driver)),
            handle: Handle {
                spawner,
                io_handle,
                time_handle,
                clock,
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
        /// let rt = runtime::Builder::new()
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
        /// let rt = runtime::Builder::new()
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

cfg_rt_core! {
    impl Builder {
        /// Sets runtime to use a simpler scheduler that runs all tasks on the current-thread.
        ///
        /// The executor and all necessary drivers will all be run on the current
        /// thread during `block_on` calls.
        ///
        /// See also [the module level documentation][1], which has a section on scheduler
        /// types.
        ///
        /// [1]: index.html#runtime-configurations
        pub fn basic_scheduler(&mut self) -> &mut Self {
            self.kind = Kind::Basic;
            self
        }

        fn build_basic_runtime(&mut self) -> io::Result<Runtime> {
            use crate::runtime::{BasicScheduler, Kind};

            let clock = time::create_clock();

            // Create I/O driver
            let (io_driver, io_handle) = io::create_driver(self.enable_io)?;

            let (driver, time_handle) = time::create_driver(self.enable_time, io_driver, clock.clone());

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
                kind: Kind::Basic(scheduler),
                handle: Handle {
                    spawner,
                    io_handle,
                    time_handle,
                    clock,
                    blocking_spawner,
                },
                blocking_pool,
            })
        }
    }
}

cfg_rt_threaded! {
    impl Builder {
        /// Sets runtime to use a multi-threaded scheduler for executing tasks.
        ///
        /// See also [the module level documentation][1], which has a section on scheduler
        /// types.
        ///
        /// [1]: index.html#runtime-configurations
        pub fn threaded_scheduler(&mut self) -> &mut Self {
            self.kind = Kind::ThreadPool;
            self
        }

        fn build_threaded_runtime(&mut self) -> io::Result<Runtime> {
            use crate::runtime::{Kind, ThreadPool};
            use crate::runtime::park::Parker;

            let core_threads = self.core_threads.unwrap_or_else(crate::loom::sys::num_cpus);
            assert!(core_threads <= self.max_threads, "Core threads number cannot be above max limit");

            let clock = time::create_clock();

            let (io_driver, io_handle) = io::create_driver(self.enable_io)?;
            let (driver, time_handle) = time::create_driver(self.enable_time, io_driver, clock.clone());
            let (scheduler, launch) = ThreadPool::new(core_threads, Parker::new(driver));
            let spawner = Spawner::ThreadPool(scheduler.spawner().clone());

            // Create the blocking pool
            let blocking_pool = blocking::create_blocking_pool(self, self.max_threads);
            let blocking_spawner = blocking_pool.spawner().clone();

            // Create the runtime handle
            let handle = Handle {
                spawner,
                io_handle,
                time_handle,
                clock,
                blocking_spawner,
            };

            // Spawn the thread pool workers
            handle.enter(|| launch.launch());

            Ok(Runtime {
                kind: Kind::ThreadPool(scheduler),
                handle,
                blocking_pool,
            })
        }
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Builder {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Builder")
            .field("kind", &self.kind)
            .field("core_threads", &self.core_threads)
            .field("max_threads", &self.max_threads)
            .field("thread_name", &self.thread_name)
            .field("thread_stack_size", &self.thread_stack_size)
            .field("after_start", &self.after_start.as_ref().map(|_| "..."))
            .field("before_stop", &self.after_start.as_ref().map(|_| "..."))
            .finish()
    }
}
