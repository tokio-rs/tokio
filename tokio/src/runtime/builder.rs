use crate::loom::sync::Arc;
#[cfg(feature = "blocking")]
use crate::runtime::blocking;
use crate::runtime::{io, timer, Runtime};

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
/// use tokio::timer::clock::Clock;
///
/// fn main() {
///     // build Runtime
///     let runtime = Builder::new()
///         .clock(Clock::system())
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
    thread_name: String,

    /// Stack size used for threads spawned by the runtime.
    thread_stack_size: Option<usize>,

    /// Callback to run after each thread starts.
    after_start: Option<Callback>,

    /// To run before each worker thread stops
    before_stop: Option<Callback>,

    /// The clock to use
    clock: timer::Clock,
}

#[derive(Debug)]
enum Kind {
    Shell,
    #[cfg(feature = "rt-current-thread")]
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

            // Default clock
            clock: timer::Clock::default(),
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
    /// The network driver, timer, and executor will all be run on the current
    /// thread during `block_on` calls.
    #[cfg(feature = "rt-current-thread")]
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

    /// Set the `Clock` instance that will be used by the runtime.
    pub fn clock(&mut self, clock: timer::Clock) -> &mut Self {
        self.clock = clock;
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
            #[cfg(feature = "rt-current-thread")]
            Kind::CurrentThread => self.build_current_thread(),
            #[cfg(feature = "rt-full")]
            Kind::ThreadPool => self.build_threadpool(),
        }
    }

    fn build_shell(&mut self) -> io::Result<Runtime> {
        use crate::runtime::Kind;

        // Create network driver
        let (net, handle) = io::create()?;
        let net_handles = vec![handle];

        let (_timer, handle) = timer::create(net, self.clock.clone());
        let timer_handles = vec![handle];

        Ok(Runtime {
            kind: Kind::Shell,
            net_handles,
            timer_handles,
            #[cfg(feature = "blocking")]
            blocking_pool: self.build_blocking_pool().into(),
        })
    }

    #[cfg(feature = "rt-current-thread")]
    fn build_current_thread(&mut self) -> io::Result<Runtime> {
        use crate::runtime::{CurrentThread, Kind};

        // Create network driver
        let (net, handle) = io::create()?;
        let net_handles = vec![handle];

        let (timer, handle) = timer::create(net, self.clock.clone());
        let timer_handles = vec![handle];

        // And now put a single-threaded executor on top of the timer. When
        // there are no futures ready to do something, it'll let the timer or
        // the reactor to generate some new stimuli for the futures to continue
        // in their life.
        let executor = CurrentThread::new(timer);

        // Blocking pool
        let blocking_pool = self.build_blocking_pool();

        Ok(Runtime {
            kind: Kind::CurrentThread(executor),
            net_handles,
            timer_handles,
            blocking_pool: blocking_pool.into(),
        })
    }

    #[cfg(feature = "rt-full")]
    fn build_threadpool(&mut self) -> io::Result<Runtime> {
        use crate::runtime::{Kind, ThreadPool};
        use crate::timer::clock;
        use std::sync::Mutex;

        let mut net_handles = Vec::new();
        let mut timer_handles = Vec::new();
        let mut timers = Vec::new();

        for _ in 0..self.num_threads {
            // Create network driver and handle
            let (net, handle) = io::create()?;
            net_handles.push(handle);

            // Create a new timer.
            let (timer, handle) = timer::create(net, self.clock.clone());
            timer_handles.push(handle);
            timers.push(Mutex::new(Some(timer)));
        }

        // Get a handle to the clock for the runtime.
        let clock = self.clock.clone();

        // Create the blocking pool
        let blocking_pool = self.build_blocking_pool();

        let pool = {
            let net_handles = net_handles.clone();
            let timer_handles = timer_handles.clone();

            let after_start = self.after_start.clone();
            let before_stop = self.before_stop.clone();

            let around_worker = Arc::new(Box::new(move |index, next: &mut dyn FnMut()| {
                // Configure the network driver
                let _net = io::set_default(&net_handles[index]);

                // Configure the clock
                clock::with_default(&clock, || {
                    // Configure the timer
                    let _timer = timer::set_default(&timer_handles[index]);

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
                blocking_pool.clone(),
                around_worker,
                move |index| timers[index].lock().unwrap().take().unwrap(),
            )
        };

        Ok(Runtime {
            kind: Kind::ThreadPool(pool),
            net_handles,
            timer_handles,
            blocking_pool: blocking_pool.into(),
        })
    }

    #[cfg(feature = "blocking")]
    fn build_blocking_pool(&self) -> Arc<blocking::Pool> {
        // Create the blocking pool
        blocking::Pool::new(self.thread_name.clone(), self.thread_stack_size)
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
            .field("clock", &self.clock)
            .finish()
    }
}
