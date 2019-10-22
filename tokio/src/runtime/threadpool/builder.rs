use super::{Inner, Runtime};
use crate::timer::clock::{self, Clock};
use crate::timer::timer::{self, Timer};

use tokio_executor::thread_pool;
use tokio_net::driver::{self, Reactor};

use tracing_core as trace;
use std::{fmt, io};
use std::sync::{Arc, Mutex};

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
///         .name_prefix("my-custom-name-")
///         .stack_size(3 * 1024 * 1024)
///         .build()
///         .unwrap();
///
///     // use runtime ...
/// }
/// ```
pub struct Builder {
    /// Thread pool specific builder
    thread_pool_builder: thread_pool::Builder,

    /// The number of worker threads
    num_threads: usize,

    /// To run after each worker thread starts
    after_start: Option<Arc<dyn Fn() + Send + Sync>>,

    /// To run before each worker thread stops
    before_stop: Option<Arc<dyn Fn() + Send + Sync>>,

    /// The clock to use
    clock: Clock,
}

impl Builder {
    /// Returns a new runtime builder initialized with default configuration
    /// values.
    ///
    /// Configuration methods can be chained on the return value.
    pub fn new() -> Builder {
        let num_threads = num_cpus::get().max(1);

        let mut thread_pool_builder = thread_pool::Builder::new();
        thread_pool_builder
            .name_prefix("tokio-runtime-worker-")
            .num_threads(num_threads);

        Builder {
            thread_pool_builder,
            num_threads,
            after_start: None,
            before_stop: None,
            clock: Clock::new(),
        }
    }

    /// Set the `Clock` instance that will be used by the runtime.
    pub fn clock(&mut self, clock: Clock) -> &mut Self {
        self.clock = clock;
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
        self.thread_pool_builder.num_threads(val);
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
    /// # use tokio::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new()
    ///     .name_prefix("my-pool-")
    ///     .build();
    /// # }
    /// ```
    pub fn name_prefix<S: Into<String>>(&mut self, val: S) -> &mut Self {
        self.thread_pool_builder.name_prefix(val);
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
    ///     .stack_size(32 * 1024)
    ///     .build();
    /// # }
    /// ```
    pub fn stack_size(&mut self, val: usize) -> &mut Self {
        self.thread_pool_builder.stack_size(val);
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
    /// let thread_pool = runtime::Builder::new()
    ///     .after_start(|| {
    ///         println!("thread started");
    ///     })
    ///     .build();
    /// # }
    /// ```
    pub fn after_start<F>(&mut self, f: F) -> &mut Self
        where F: Fn() + Send + Sync + 'static
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
    /// let thread_pool = runtime::Builder::new()
    ///     .before_stop(|| {
    ///         println!("thread stopping");
    ///     })
    ///     .build();
    /// # }
    /// ```
    pub fn before_stop<F>(&mut self, f: F) -> &mut Self
        where F: Fn() + Send + Sync + 'static
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
    /// # use tokio::runtime::Builder;
    /// # pub fn main() {
    /// let runtime = Builder::new().build().unwrap();
    /// // ... call runtime.run(...)
    /// # let _ = runtime;
    /// # }
    /// ```
    pub fn build(&mut self) -> io::Result<Runtime> {
        let mut reactor_handles = Vec::new();
        let mut timer_handles = Vec::new();
        let mut timers = Vec::new();

        for _ in 0..self.num_threads {
            // Create a new reactor.
            let reactor = Reactor::new()?;
            reactor_handles.push(reactor.handle());

            // Create a new timer.
            let timer = Timer::new_with_clock(reactor, self.clock.clone());
            timer_handles.push(timer.handle());
            timers.push(Mutex::new(Some(timer)));
        }

        // Get a handle to the clock for the runtime.
        let clock = self.clock.clone();

        // Get the current trace dispatcher.
        // TODO(eliza): when `tokio-trace-core` is stable enough to take a
        // public API dependency, we should allow users to set a custom
        // subscriber for the runtime.
        let dispatch = trace::dispatcher::get_default(trace::Dispatch::clone);
        let trace = dispatch.clone();

        let around_reactor_handles = reactor_handles.clone();
        let around_timer_handles = timer_handles.clone();

        let after_start = self.after_start.clone();
        let before_stop = self.before_stop.clone();

        let pool = self
            .thread_pool_builder
            .around_worker(move |index, next| {
                let _reactor = driver::set_default(&around_reactor_handles[index]);
                clock::with_default(&clock, || {
                    let _timer = timer::set_default(&around_timer_handles[index]);
                    trace::dispatcher::with_default(&dispatch, || {
                        if let Some(after_start) = after_start.as_ref() {
                            after_start();
                        }

                        next();

                        if let Some(before_stop) = before_stop.as_ref() {
                            before_stop();
                        }
                    })
                })
            })
            .build_with_park(move |index| {
                timers[index]
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap()
            });

        Ok(Runtime {
            inner: Some(Inner {
                pool,
                reactor_handles,
                timer_handles,
                trace,
            }),
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
            .field("thread_pool_builder", &self.thread_pool_builder)
            .field("after_start", &self.after_start.as_ref().map(|_| "..."))
            .finish()
    }
}
