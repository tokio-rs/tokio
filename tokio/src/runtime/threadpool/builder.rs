use super::{Inner, Runtime};

use reactor::Reactor;

use std::io;
use std::sync::Mutex;
use std::time::Duration;
use std::any::Any;

use num_cpus;
use tokio_reactor;
use tokio_threadpool::Builder as ThreadPoolBuilder;
use tokio_timer::clock::{self, Clock};
use tokio_timer::timer::{self, Timer};

#[cfg(feature = "experimental-tracing")]
use tracing_core as trace;

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
/// extern crate tokio;
/// extern crate tokio_timer;
///
/// use std::time::Duration;
///
/// use tokio::runtime::Builder;
/// use tokio_timer::clock::Clock;
///
/// fn main() {
///     // build Runtime
///     let mut runtime = Builder::new()
///         .blocking_threads(4)
///         .clock(Clock::system())
///         .core_threads(4)
///         .keep_alive(Some(Duration::from_secs(60)))
///         .name_prefix("my-custom-name-")
///         .stack_size(3 * 1024 * 1024)
///         .build()
///         .unwrap();
///
///     // use runtime ...
/// }
/// ```
#[derive(Debug)]
pub struct Builder {
    /// Thread pool specific builder
    threadpool_builder: ThreadPoolBuilder,

    /// The number of worker threads
    core_threads: usize,

    /// The clock to use
    clock: Clock,
}

impl Builder {
    /// Returns a new runtime builder initialized with default configuration
    /// values.
    ///
    /// Configuration methods can be chained on the return value.
    pub fn new() -> Builder {
        let core_threads = num_cpus::get().max(1);

        let mut threadpool_builder = ThreadPoolBuilder::new();
        threadpool_builder.name_prefix("tokio-runtime-worker-");
        threadpool_builder.pool_size(core_threads);

        Builder {
            threadpool_builder,
            core_threads,
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
        since = "0.1.9",
        note = "use the `core_threads`, `blocking_threads`, `name_prefix`, \
                `keep_alive`, and `stack_size` functions on `runtime::Builder`, \
                instead")]
    #[doc(hidden)]
    pub fn threadpool_builder(&mut self, val: ThreadPoolBuilder) -> &mut Self {
        self.threadpool_builder = val;
        self
    }

    /// Sets a callback to handle panics in futures.
    ///
    /// The callback is triggered when a panic during a future bubbles up to
    /// Tokio. By default Tokio catches these panics, and they will be ignored.
    /// The parameter passed to this callback is the same error value returned
    /// from `std::panic::catch_unwind()`. To abort the process on panics, use
    /// `std::panic::resume_unwind()` in this callback as shown below.
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
    ///     .panic_handler(|err| std::panic::resume_unwind(err))
    ///     .build()
    ///     .unwrap();
    /// # }
    /// ```
    pub fn panic_handler<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(Box<Any + Send>) + Send + Sync + 'static,
    {
        self.threadpool_builder.panic_handler(f);
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
        self.core_threads = val;
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

    /// Set the worker thread keep alive duration for threads in the `Runtime`'s
    /// thread pool.
    ///
    /// If set, a worker thread will wait for up to the specified duration for
    /// work, at which point the thread will shutdown. When work becomes
    /// available, a new thread will eventually be spawned to replace the one
    /// that shut down.
    ///
    /// When the value is `None`, the thread will wait for work forever.
    ///
    /// The default value is `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # extern crate futures;
    /// # use tokio::runtime;
    /// use std::time::Duration;
    ///
    /// # pub fn main() {
    /// let mut rt = runtime::Builder::new()
    ///     .keep_alive(Some(Duration::from_secs(30)))
    ///     .build();
    /// # }
    /// ```
    pub fn keep_alive(&mut self, val: Option<Duration>) -> &mut Self {
        self.threadpool_builder.keep_alive(val);
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

    /// Execute function `f` after each thread is started but before it starts
    /// doing work.
    ///
    /// This is intended for bookkeeping and monitoring use cases.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # extern crate futures;
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
        self.threadpool_builder.after_start(f);
        self
    }

    /// Execute function `f` before each thread stops.
    ///
    /// This is intended for bookkeeping and monitoring use cases.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # extern crate futures;
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
        self.threadpool_builder.before_stop(f);
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
        // TODO(stjepang): Once we remove the `threadpool_builder` method, remove this line too.
        self.threadpool_builder.pool_size(self.core_threads);

        let mut reactor_handles = Vec::new();
        let mut timer_handles = Vec::new();
        let mut timers = Vec::new();

        for _ in 0..self.core_threads {
            // Create a new reactor.
            let reactor = Reactor::new()?;
            reactor_handles.push(reactor.handle());

            // Create a new timer.
            let timer = Timer::new_with_now(reactor, self.clock.clone());
            timer_handles.push(timer.handle());
            timers.push(Mutex::new(Some(timer)));
        }

        // Get a handle to the clock for the runtime.
        let clock = self.clock.clone();

        // Get the current trace dispatcher.
        // TODO(eliza): when `tracing-core` is stable enough to take a
        // public API dependency, we should allow users to set a custom
        // subscriber for the runtime.
        #[cfg(feature = "experimental-tracing")]
        let dispatch = trace::dispatcher::get_default(trace::Dispatch::clone);

        let pool = self
            .threadpool_builder
            .around_worker(move |w, enter| {
                let index = w.id().to_usize();

                tokio_reactor::with_default(&reactor_handles[index], enter, |enter| {
                    clock::with_default(&clock, enter, |enter| {
                        timer::with_default(&timer_handles[index], enter, |_| {

                            #[cfg(feature = "experimental-tracing")]
                            trace::dispatcher::with_default(&dispatch, || {
                                w.run();
                            });

                            #[cfg(not(feature = "experimental-tracing"))]
                            w.run();
                        });
                    })
                });
            })
            .custom_park(move |worker_id| {
                let index = worker_id.to_usize();

                timers[index]
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap()
            })
            .build();

        // To support deprecated `reactor()` function
        let reactor = Reactor::new()?;
        let reactor_handle = reactor.handle();

        Ok(Runtime {
            inner: Some(Inner {
                reactor_handle,
                reactor: Mutex::new(Some(reactor)),
                pool,
            }),
        })
    }
}
