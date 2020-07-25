use crate::runtime::{blocking, context, io, time, Spawner};
use std::{error, fmt};

cfg_blocking! {
    use crate::runtime::task;
    use crate::runtime::blocking::task::BlockingTask;
}

cfg_rt_core! {
    use crate::task::JoinHandle;

    use std::future::Future;
}

/// Handle to the runtime.
///
/// The handle is internally reference-counted and can be freely cloned. A handle can be
/// obtained using the [`Runtime::handle`] method.
///
/// [`Runtime::handle`]: crate::runtime::Runtime::handle()
#[derive(Debug, Clone)]
pub struct Handle {
    pub(super) spawner: Spawner,

    /// Handles to the I/O drivers
    pub(super) io_handle: io::Handle,

    /// Handles to the time drivers
    pub(super) time_handle: time::Handle,

    /// Source of `Instant::now()`
    pub(super) clock: time::Clock,

    /// Blocking pool spawner
    pub(super) blocking_spawner: blocking::Spawner,
}

impl Handle {
    /// Enter the runtime context. This allows you to construct types that must
    /// have an executor available on creation such as [`Delay`] or [`TcpStream`].
    /// It will also allow you to call methods such as [`tokio::spawn`].
    ///
    /// This function is also available as [`Runtime::enter`].
    ///
    /// [`Delay`]: struct@crate::time::Delay
    /// [`TcpStream`]: struct@crate::net::TcpStream
    /// [`Runtime::enter`]: fn@crate::runtime::Runtime::enter
    /// [`tokio::spawn`]: fn@crate::spawn
    ///
    /// # Example
    ///
    /// ```
    /// use tokio::runtime::Runtime;
    ///
    /// fn function_that_spawns(msg: String) {
    ///     // Had we not used `handle.enter` below, this would panic.
    ///     tokio::spawn(async move {
    ///         println!("{}", msg);
    ///     });
    /// }
    ///
    /// fn main() {
    ///     let rt = Runtime::new().unwrap();
    ///     let handle = rt.handle().clone();
    ///
    ///     let s = "Hello World!".to_string();
    ///
    ///     // By entering the context, we tie `tokio::spawn` to this executor.
    ///     handle.enter(|| function_that_spawns(s));
    /// }
    /// ```
    pub fn enter<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        context::enter(self.clone(), f)
    }

    /// Returns a `Handle` view over the currently running `Runtime`
    ///
    /// # Panic
    ///
    /// This will panic if called outside the context of a Tokio runtime. That means that you must
    /// call this on one of the threads **being run by the runtime**. Calling this from within a
    /// thread created by `std::thread::spawn` (for example) will cause a panic.
    ///
    /// # Examples
    ///
    /// This can be used to obtain the handle of the surrounding runtime from an async
    /// block or function running on that runtime.
    ///
    /// ```
    /// # use std::thread;
    /// # use tokio::runtime::Runtime;
    /// # fn dox() {
    /// # let rt = Runtime::new().unwrap();
    /// # rt.spawn(async {
    /// use tokio::runtime::Handle;
    ///
    /// // Inside an async block or function.
    /// let handle = Handle::current();
    /// handle.spawn(async {
    ///     println!("now running in the existing Runtime");
    /// });
    ///
    /// # let handle =
    /// thread::spawn(move || {
    ///     // Notice that the handle is created outside of this thread and then moved in
    ///     handle.block_on(async { /* ... */ })
    ///     // This next line would cause a panic
    ///     // let handle2 = Handle::current();
    /// });
    /// # handle.join().unwrap();
    /// # });
    /// # }
    /// ```
    pub fn current() -> Self {
        context::current().expect("not currently running on the Tokio runtime.")
    }

    /// Returns a Handle view over the currently running Runtime
    ///
    /// Returns an error if no Runtime has been started
    ///
    /// Contrary to `current`, this never panics
    pub fn try_current() -> Result<Self, TryCurrentError> {
        context::current().ok_or(TryCurrentError(()))
    }
}

cfg_rt_core! {
    impl Handle {
        /// Spawns a future onto the Tokio runtime.
        ///
        /// This spawns the given future onto the runtime's executor, usually a
        /// thread pool. The thread pool is then responsible for polling the future
        /// until it completes.
        ///
        /// See [module level][mod] documentation for more details.
        ///
        /// [mod]: index.html
        ///
        /// # Examples
        ///
        /// ```
        /// use tokio::runtime::Runtime;
        ///
        /// # fn dox() {
        /// // Create the runtime
        /// let rt = Runtime::new().unwrap();
        /// let handle = rt.handle();
        ///
        /// // Spawn a future onto the runtime
        /// handle.spawn(async {
        ///     println!("now running on a worker thread");
        /// });
        /// # }
        /// ```
        ///
        /// # Panics
        ///
        /// This function will not panic unless task execution is disabled on the
        /// executor. This can only happen if the runtime was built using
        /// [`Builder`] without picking either [`basic_scheduler`] or
        /// [`threaded_scheduler`].
        ///
        /// [`Builder`]: struct@crate::runtime::Builder
        /// [`threaded_scheduler`]: fn@crate::runtime::Builder::threaded_scheduler
        /// [`basic_scheduler`]: fn@crate::runtime::Builder::basic_scheduler
        pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
        where
            F: Future + Send + 'static,
            F::Output: Send + 'static,
        {
            self.spawner.spawn(future)
        }

        /// Run a future to completion on the Tokio runtime from a synchronous
        /// context.
        ///
        /// This runs the given future on the runtime, blocking until it is
        /// complete, and yielding its resolved result. Any tasks or timers which
        /// the future spawns internally will be executed on the runtime.
        ///
        /// If the provided executor currently has no active core thread, this
        /// function might hang until a core thread is added. This is not a
        /// concern when using the [threaded scheduler], as it always has active
        /// core threads, but if you use the [basic scheduler], some other
        /// thread must currently be inside a call to [`Runtime::block_on`].
        /// See also [the module level documentation][1], which has a section on
        /// scheduler types.
        ///
        /// This method may not be called from an asynchronous context.
        ///
        /// [threaded scheduler]: fn@crate::runtime::Builder::threaded_scheduler
        /// [basic scheduler]: fn@crate::runtime::Builder::basic_scheduler
        /// [`Runtime::block_on`]: fn@crate::runtime::Runtime::block_on
        /// [1]: index.html#runtime-configurations
        ///
        /// # Panics
        ///
        /// This function panics if the provided future panics, or if called
        /// within an asynchronous execution context.
        ///
        /// # Examples
        ///
        /// Using `block_on` with the [threaded scheduler].
        ///
        /// ```
        /// use tokio::runtime::Runtime;
        /// use std::thread;
        ///
        /// // Create the runtime.
        /// //
        /// // If the rt-threaded feature is enabled, this creates a threaded
        /// // scheduler by default.
        /// let rt = Runtime::new().unwrap();
        /// let handle = rt.handle().clone();
        ///
        /// // Use the runtime from another thread.
        /// let th = thread::spawn(move || {
        ///     // Execute the future, blocking the current thread until completion.
        ///     //
        ///     // This example uses the threaded scheduler, so no concurrent call to
        ///     // `rt.block_on` is required.
        ///     handle.block_on(async {
        ///         println!("hello");
        ///     });
        /// });
        ///
        /// th.join().unwrap();
        /// ```
        ///
        /// Using the [basic scheduler] requires a concurrent call to
        /// [`Runtime::block_on`]:
        ///
        /// [threaded scheduler]: fn@crate::runtime::Builder::threaded_scheduler
        /// [basic scheduler]: fn@crate::runtime::Builder::basic_scheduler
        /// [`Runtime::block_on`]: fn@crate::runtime::Runtime::block_on
        ///
        /// ```
        /// use tokio::runtime::Builder;
        /// use tokio::sync::oneshot;
        /// use std::thread;
        ///
        /// // Create the runtime.
        /// let mut rt = Builder::new()
        ///     .enable_all()
        ///     .basic_scheduler()
        ///     .build()
        ///     .unwrap();
        ///
        /// let handle = rt.handle().clone();
        ///
        /// // Signal main thread when task has finished.
        /// let (send, recv) = oneshot::channel();
        ///
        /// // Use the runtime from another thread.
        /// let th = thread::spawn(move || {
        ///     // Execute the future, blocking the current thread until completion.
        ///     handle.block_on(async {
        ///         send.send("done").unwrap();
        ///     });
        /// });
        ///
        /// // The basic scheduler is used, so the thread above might hang if we
        /// // didn't call block_on on the rt too.
        /// rt.block_on(async {
        ///     assert_eq!(recv.await.unwrap(), "done");
        /// });
        /// # th.join().unwrap();
        /// ```
        ///
        pub fn block_on<F: Future>(&self, future: F) -> F::Output {
            self.enter(|| {
                let mut enter = crate::runtime::enter(true);
                enter.block_on(future).expect("failed to park thread")
            })
        }
    }
}

cfg_blocking! {
    impl Handle {
        /// Runs the provided closure on a thread where blocking is acceptable.
        ///
        /// In general, issuing a blocking call or performing a lot of compute in a
        /// future without yielding is not okay, as it may prevent the executor from
        /// driving other futures forward. This function runs the provided closure
        /// on a thread dedicated to blocking operations. See the [CPU-bound tasks
        /// and blocking code][blocking] section for more information.
        ///
        /// Tokio will spawn more blocking threads when they are requested through
        /// this function until the upper limit configured on the [`Builder`] is
        /// reached.  This limit is very large by default, because `spawn_blocking` is
        /// often used for various kinds of IO operations that cannot be performed
        /// asynchronously. When you run CPU-bound code using `spawn_blocking`, you
        /// should keep this large upper limit in mind; to run your CPU-bound
        /// computations on only a few threads, you should use a separate thread
        /// pool such as [rayon] rather than configuring the number of blocking
        /// threads.
        ///
        /// This function is intended for non-async operations that eventually
        /// finish on their own. If you want to spawn an ordinary thread, you should
        /// use [`thread::spawn`] instead.
        ///
        /// Closures spawned using `spawn_blocking` cannot be cancelled. When you
        /// shut down the executor, it will wait indefinitely for all blocking
        /// operations to finish. You can use [`shutdown_timeout`] to stop waiting
        /// for them after a certain timeout. Be aware that this will still not
        /// cancel the tasks — they are simply allowed to keep running after the
        /// method returns.
        ///
        /// Note that if you are using the [basic scheduler], this function will
        /// still spawn additional threads for blocking operations. The basic
        /// scheduler's single thread is only used for asynchronous code.
        ///
        /// [`Builder`]: struct@crate::runtime::Builder
        /// [blocking]: ../index.html#cpu-bound-tasks-and-blocking-code
        /// [rayon]: https://docs.rs/rayon
        /// [basic scheduler]: fn@crate::runtime::Builder::basic_scheduler
        /// [`thread::spawn`]: fn@std::thread::spawn
        /// [`shutdown_timeout`]: fn@crate::runtime::Runtime::shutdown_timeout
        ///
        /// # Examples
        ///
        /// ```
        /// use tokio::runtime::Runtime;
        ///
        /// # async fn docs() -> Result<(), Box<dyn std::error::Error>>{
        /// // Create the runtime
        /// let rt = Runtime::new().unwrap();
        /// let handle = rt.handle();
        ///
        /// let res = handle.spawn_blocking(move || {
        ///     // do some compute-heavy work or call synchronous code
        ///     "done computing"
        /// }).await?;
        ///
        /// assert_eq!(res, "done computing");
        /// # Ok(())
        /// # }
        /// ```
        pub fn spawn_blocking<F, R>(&self, f: F) -> JoinHandle<R>
        where
            F: FnOnce() -> R + Send + 'static,
            R: Send + 'static,
        {
            let (task, handle) = task::joinable(BlockingTask::new(f));
            let _ = self.blocking_spawner.spawn(task, self);
            handle
        }
    }
}

/// Error returned by `try_current` when no Runtime has been started
pub struct TryCurrentError(());

impl fmt::Debug for TryCurrentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TryCurrentError").finish()
    }
}

impl fmt::Display for TryCurrentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("no tokio Runtime has been initialized")
    }
}

impl error::Error for TryCurrentError {}
