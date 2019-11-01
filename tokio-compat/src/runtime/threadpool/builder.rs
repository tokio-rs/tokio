use super::{background, compat, Inner, Runtime};

use tokio_02::executor::thread_pool;
use tokio_02::net::driver::{self, Reactor};
use tokio_02::timer::clock::{self, Clock};
use tokio_02::timer::timer::{self, Timer};
use tokio_executor_01 as executor_01;
use tokio_reactor_01 as reactor_01;
use tokio_timer_02 as timer_02;

use num_cpus;
use std::io;
use std::sync::{Arc, Barrier, Mutex, RwLock};

/// Builds a compatibility runtime with custom configuration values.
///
/// This runtime is compatible with code using both the current release version
/// of `tokio` (0.1) and with legacy code using `tokio` 0.1.
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
///
/// use tokio_compat::runtime::Builder;
/// use tokio_02::timer::clock::Clock;
///
/// fn main() {
///     // build Runtime
///     let runtime = Builder::new()
///         .clock(Clock::system())
///         .core_threads(4)
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
    threadpool_builder: thread_pool::Builder,

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

        let mut threadpool_builder = thread_pool::Builder::new();
        threadpool_builder.name("tokio-runtime-worker");
        threadpool_builder.num_threads(core_threads);

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
        self.threadpool_builder.num_threads(val);
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
        self.threadpool_builder.name(val);
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
        self.threadpool_builder.num_threads(self.core_threads);

        let mut reactor_handles = Vec::new();
        let mut timer_handles = Vec::new();
        let mut timers = Vec::new();

        for _ in 0..self.core_threads {
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
        let dispatch = tracing_core::dispatcher::get_default(tracing_core::Dispatch::clone);
        let trace = dispatch.clone();

        let background = background::spawn(&clock)?;
        let compat_bg = compat::Background::spawn(&clock)?;
        let compat_reactor = compat_bg.reactor().clone();
        let compat_timer = compat_bg.timer().clone();

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
        let compat_sender = Arc::new((RwLock::new(None), Barrier::new(self.core_threads + 1)));
        let compat_sender2 = compat_sender.clone();

        let pool = self
            .threadpool_builder
            .around_worker(move |index, work| {
                let mut enter = executor_01::enter().unwrap();
                // We need the threadpool's sender to set up the default tokio
                // 0.1 executor.
                let (compat_sender, sender_ready) = &*compat_sender2;
                // Wait for the sender to be set.
                sender_ready.wait();
                let mut compat_sender = compat_sender
                    .read()
                    .unwrap()
                    .clone()
                    .expect("compat executor needs to be set before the pool is run!");

                // Set the default tokio 0.1 reactor to the background compat reactor.
                reactor_01::with_default(&compat_reactor, &mut enter, |enter| {
                    // Set the default tokio 0.2 reactor to this worker thread's
                    // reactor.
                    let _reactor = driver::set_default(&reactor_handles[index]);
                    clock::with_default(&clock, || {
                        // Set up a default timer for tokio 0.1 compat.
                        timer_02::with_default(&compat_timer, enter, |enter| {
                            let _timer = timer::set_default(&timer_handles[index]);
                            tracing_core::dispatcher::with_default(&dispatch, || {
                                // Set the default executor for tokio 0.1 compat.
                                executor_01::with_default(&mut compat_sender, enter, |_enter| {
                                    work();
                                })
                            })
                        })
                    })
                });
            })
            .build_with_park(move |index| timers[index].lock().unwrap().take().unwrap());

        let (idle, idle_rx) = super::idle::Idle::new();
        let runtime = Runtime {
            inner: Some(Inner {
                pool,
                background,
                compat_bg,
                trace,
            }),
            idle_rx,
            idle,
        };

        // Set the tokio 0.1 executor to be used by the worker threads.
        let (compat_sender, sender_ready) = &*compat_sender;
        *compat_sender.write().unwrap() = Some(runtime.spawner());
        sender_ready.wait();

        Ok(runtime)
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}
