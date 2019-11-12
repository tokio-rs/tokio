use crate::loom::sync::Arc;
use crate::runtime::handle::{self, Handle};
use crate::runtime::shell::Shell;
use crate::runtime::{blocking, io, time, Runtime};

use std::fmt;

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
///     // build Runtime
///     let runtime = Builder::new()
///         .num_threads(4)
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

    /// The number of worker threads.
    ///
    /// Only used when not using the current-thread executor.
    num_threads: usize,

    /// Name used for threads spawned by the runtime.
    pub(super) thread_name: String,

    /// Stack size used for threads spawned by the runtime.
    pub(super) thread_stack_size: Option<usize>,

    /// Callback to run after each thread starts.
    after_start: Option<Callback>,

    /// To run before each worker thread stops
    before_stop: Option<Callback>,
}

#[derive(Debug)]
enum Kind {
    Shell,
    #[cfg(feature = "rt-core")]
    CurrentThread,
    #[cfg(feature = "rt-full")]
    ThreadPool,
}

#[cfg(not(loom))]
type Callback = Arc<dyn Fn() + Send + Sync>;

#[cfg(loom)]
type Callback = Arc<Box<dyn Fn() + Send + Sync>>;

impl Builder {
    /// Returns a new runtime builder initialized with default configuration
    /// values.
    ///
    /// Configuration methods can be chained on the return value.
    pub fn new() -> Builder {
        Builder {
            // No task execution by default
            kind: Kind::Shell,

            // Default to use an equal number of threads to number of CPU cores
            num_threads: crate::loom::sys::num_cpus(),

            // Default thread name
            thread_name: "tokio-runtime-worker".into(),

            // Do not set a stack size by default
            thread_stack_size: None,

            // No worker thread callbacks
            after_start: None,
            before_stop: None,
        }
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
    /// # use tokio::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new()
    ///     .num_threads(4)
    ///     .build()
    ///     .unwrap();
    /// # }
    /// ```
    pub fn num_threads(&mut self, val: usize) -> &mut Self {
        self.num_threads = val;
        self
    }

    /// Use only the current thread for executing tasks.
    ///
    /// The executor and all necessary drivers will all be run on the current
    /// thread during `block_on` calls.
    #[cfg(feature = "rt-core")]
    pub fn current_thread(&mut self) -> &mut Self {
        self.kind = Kind::CurrentThread;
        self
    }

    /// Use a thread-pool for executing tasks.
    #[cfg(feature = "rt-full")]
    pub fn thread_pool(&mut self) -> &mut Self {
        self.kind = Kind::ThreadPool;
        self
    }

    /// Set name of threads spawned by the `Runtime`'s thread pool.
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
    /// # use tokio::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new()
    ///     .thread_stack_size(32 * 1024)
    ///     .build();
    /// # }
    /// ```
    pub fn thread_stack_size(&mut self, val: usize) -> &mut Self {
        self.thread_stack_size = Some(val);
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
    /// # use tokio::runtime;
    ///
    /// # pub fn main() {
    /// let runtime = runtime::Builder::new()
    ///     .after_start(|| {
    ///         println!("thread started");
    ///     })
    ///     .build();
    /// # }
    /// ```
    #[cfg(not(loom))]
    pub fn after_start<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.after_start = Some(Arc::new(f));
        self
    }

    /// Execute function `f` before each thread stops.
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
    ///     .before_stop(|| {
    ///         println!("thread stopping");
    ///     })
    ///     .build();
    /// # }
    /// ```
    #[cfg(not(loom))]
    pub fn before_stop<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.before_stop = Some(Arc::new(f));
        self
    }

    /// Create the configured `Runtime`.
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
            Kind::Shell => self.build_shell(),
            #[cfg(feature = "rt-core")]
            Kind::CurrentThread => self.build_current_thread(),
            #[cfg(feature = "rt-full")]
            Kind::ThreadPool => self.build_threadpool(),
        }
    }

    fn build_shell(&mut self) -> io::Result<Runtime> {
        use crate::runtime::Kind;

        let clock = time::create_clock();

        // Create I/O driver
        let (io_driver, handle) = io::create_driver()?;
        let io_handles = vec![handle];

        let (driver, handle) = time::create_driver(io_driver, clock.clone());
        let time_handles = vec![handle];

        let blocking_pool = blocking::create_blocking_pool(self);
        let blocking_spawner = blocking_pool.spawner().clone();

        Ok(Runtime {
            kind: Kind::Shell(Shell::new(driver)),
            handle: Handle {
                kind: handle::Kind::Shell,
                io_handles,
                time_handles,
                clock,
                blocking_spawner,
            },
            blocking_pool,
        })
    }

    #[cfg(feature = "rt-core")]
    fn build_current_thread(&mut self) -> io::Result<Runtime> {
        use crate::runtime::{CurrentThread, Kind};

        let clock = time::create_clock();

        // Create I/O driver
        let (io_driver, handle) = io::create_driver()?;
        let io_handles = vec![handle];

        let (driver, handle) = time::create_driver(io_driver, clock.clone());
        let time_handles = vec![handle];

        // And now put a single-threaded scheduler on top of the timer. When
        // there are no futures ready to do something, it'll let the timer or
        // the reactor to generate some new stimuli for the futures to continue
        // in their life.
        let scheduler = CurrentThread::new(driver);
        let spawner = scheduler.spawner();

        // Blocking pool
        let blocking_pool = blocking::create_blocking_pool(self);
        let blocking_spawner = blocking_pool.spawner().clone();

        Ok(Runtime {
            kind: Kind::CurrentThread(scheduler),
            handle: Handle {
                kind: handle::Kind::CurrentThread(spawner),
                io_handles,
                time_handles,
                clock,
                blocking_spawner,
            },
            blocking_pool,
        })
    }

    #[cfg(feature = "rt-full")]
    fn build_threadpool(&mut self) -> io::Result<Runtime> {
        use crate::runtime::{Kind, ThreadPool};
        use std::sync::Mutex;

        let clock = time::create_clock();

        let mut io_handles = Vec::new();
        let mut time_handles = Vec::new();
        let mut drivers = Vec::new();

        for _ in 0..self.num_threads {
            // Create I/O driver and handle
            let (io_driver, handle) = io::create_driver()?;
            io_handles.push(handle);

            // Create a new timer.
            let (time_driver, handle) = time::create_driver(io_driver, clock.clone());
            time_handles.push(handle);
            drivers.push(Mutex::new(Some(time_driver)));
        }

        // Create the blocking pool
        let blocking_pool = blocking::create_blocking_pool(self);
        let blocking_spawner = blocking_pool.spawner().clone();

        let scheduler = {
            let clock = clock.clone();
            let io_handles = io_handles.clone();
            let time_handles = time_handles.clone();

            let after_start = self.after_start.clone();
            let before_stop = self.before_stop.clone();

            let around_worker = Arc::new(Box::new(move |index, next: &mut dyn FnMut()| {
                // Configure the I/O driver
                let _io = io::set_default(&io_handles[index]);

                // Configure time
                time::with_default(&time_handles[index], &clock, || {
                    // Call the start callback
                    if let Some(after_start) = after_start.as_ref() {
                        after_start();
                    }

                    // Run the worker
                    next();

                    // Call the after call back
                    if let Some(before_stop) = before_stop.as_ref() {
                        before_stop();
                    }
                })
            })
                as Box<dyn Fn(usize, &mut dyn FnMut()) + Send + Sync>);

            ThreadPool::new(
                self.num_threads,
                blocking_pool.spawner().clone(),
                around_worker,
                move |index| drivers[index].lock().unwrap().take().unwrap(),
            )
        };

        let spawner = scheduler.spawner().clone();

        Ok(Runtime {
            kind: Kind::ThreadPool(scheduler),
            handle: Handle {
                kind: handle::Kind::ThreadPool(spawner),
                io_handles,
                time_handles,
                clock,
                blocking_spawner,
            },
            blocking_pool,
        })
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
            .field("num_threads", &self.num_threads)
            .field("thread_name", &self.thread_name)
            .field("thread_stack_size", &self.thread_stack_size)
            .field("after_start", &self.after_start.as_ref().map(|_| "..."))
            .field("before_stop", &self.after_start.as_ref().map(|_| "..."))
            .finish()
    }
}
