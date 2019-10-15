use super::{background, compat::Compat, Inner, Runtime};

use tokio_executor::threadpool;
use tokio_net::driver::{self, Reactor};
use tokio_timer::clock::{self, Clock};
use tokio_timer::timer::{self, Timer};

use num_cpus;
use std::any::Any;
use std::io;
use std::sync::{Arc, Mutex, RwLock};
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
/// [`build`]: #method.build
/// [`Builder::new`]: #method.new
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use tokio_compat::runtime::Builder;
/// use tokio_timer::clock::Clock;
///
/// fn main() {
///     // build Runtime
///     let runtime = Builder::new()
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
    threadpool_builder: threadpool::Builder,

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

        let mut threadpool_builder = threadpool::Builder::new();
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
    /// # use tokio_compat::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new()
    ///     .panic_handler(|err| std::panic::resume_unwind(err))
    ///     .build()
    ///     .unwrap();
    /// # }
    /// ```
    pub fn panic_handler<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(Box<dyn Any + Send>) + Send + Sync + 'static,
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
    /// # use tokio_compat::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new()
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
    /// # use tokio_compat::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new()
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
    /// # use tokio_compat::runtime;
    /// use std::time::Duration;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new()
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
    /// # use tokio_compat::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new()
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
    /// # use tokio_compat::runtime;
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new()
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
    /// # use tokio_compat::runtime;
    ///
    /// # pub fn main() {
    /// let runtime = runtime::Builder::new()
    ///     .after_start(|| {
    ///         println!("thread started");
    ///     })
    ///     .build();
    /// # }
    /// ```
    pub fn after_start<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
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
    /// # use tokio_compat::runtime;
    ///
    /// # pub fn main() {
    /// let runtime = runtime::Builder::new()
    ///     .before_stop(|| {
    ///         println!("thread stopping");
    ///     })
    ///     .build();
    /// # }
    /// ```
    pub fn before_stop<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn() + Send + Sync + 'static,
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
    /// # use tokio_compat::runtime::Builder;
    /// # pub fn main() {
    /// let runtime = Builder::new().build().unwrap();
    /// // ... call runtime.run(...)
    /// # let _ = runtime;
    /// # }
    /// ```
    pub fn build(&mut self) -> io::Result<Runtime> {
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
        let dispatch = tracing_core::dispatcher::get_default(tracing_core::Dispatch::clone);
        let trace = dispatch.clone();

        let background = background::spawn(&clock)?;
        let Compat {
            compat_reactor,
            compat_timer,
            compat_bg,
        } = Compat::spawn(&clock)?;

        // The `tokio` 0.2 default executor for the worker threads will be set
        // by the threadpool itself, but in order to set a default executor for
        // `tokio-executor` 0.1 compatibility, we need a `Sender` for the pool
        // in the `around_worker` closure.
        //
        // Unfortunately, we can't get a sender until the pool is constructed,
        // which requires the `around_worker` closure. As a workaround, we can
        // an `Arc<RwLock>`, which can be moved into the closure, and then be
        // set once the pool is constructed. Since the closures won't _run_
        // until we actually try to run futures on the pool, it's okay to set
        // the sender after constructing the pool.
        //
        // The lock is only acquired in `around_worker`; once it is acquired the
        // sender is cloned, and the lock doesn't need to be acquired to spawn a
        // future. Since we only use it when creating the pool, there shouldn't
        // be much of a performance impact.
        let compat_sender = Arc::new(RwLock::new(None));
        let compat_sender2 = compat_sender.clone();

        let pool = self
            .threadpool_builder
            .around_worker(move |w| {
                let index = w.id().to_usize();
                let mut enter = old_executor::enter().unwrap();
                // We need the threadpool's sender to set up the default tokio
                // 0.1 executor.
                let sender_lock = compat_sender2.read().unwrap();
                let mut compat_sender = sender_lock
                    .clone()
                    .expect("compat executor be set before the pool is run!");

                // Set the default tokio 0.1 reactor to the background compat reactor.
                old_reactor::with_default(&compat_reactor, &mut enter, |enter| {
                    // Set the default tokio 0.2 reactor to this worker thread's
                    // reactor.
                    let _reactor = driver::set_default(&reactor_handles[index]);
                    clock::with_default(&clock, || {
                        // Set up a default timer for tokio 0.1 compat.
                        old_timer::with_default(&compat_timer, enter, |enter| {
                            let _timer = timer::set_default(&timer_handles[index]);
                            tracing_core::dispatcher::with_default(&dispatch, || {
                                // Set the default executor for tokio 0.1 compat.
                                old_executor::with_default(&mut compat_sender, enter, |_enter| {
                                    w.run();
                                })
                            })
                        })
                    })
                });
            })
            .custom_park(move |worker_id| {
                let index = worker_id.to_usize();

                timers[index].lock().unwrap().take().unwrap()
            })
            .build();

        *compat_sender.write().unwrap() = Some(super::CompatSender(pool.sender().clone()));

        Ok(Runtime {
            inner: Some(Inner {
                pool,
                background,
                compat_bg,
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
