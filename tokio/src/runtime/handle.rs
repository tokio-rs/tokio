use crate::runtime::blocking::{BlockingTask, NoopSchedule};
use crate::runtime::task::{self, JoinHandle};
use crate::runtime::{blocking, context, driver, Spawner};
use crate::util::error::{CONTEXT_MISSING_ERROR, THREAD_LOCAL_DESTROYED_ERROR};

use std::future::Future;
use std::marker::PhantomData;
use std::{error, fmt};

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
    #[cfg_attr(
        not(any(feature = "net", feature = "process", all(unix, feature = "signal"))),
        allow(dead_code)
    )]
    pub(super) io_handle: driver::IoHandle,

    /// Handles to the signal drivers
    #[cfg_attr(
        not(any(feature = "signal", all(unix, feature = "process"))),
        allow(dead_code)
    )]
    pub(super) signal_handle: driver::SignalHandle,

    /// Handles to the time drivers
    #[cfg_attr(not(feature = "time"), allow(dead_code))]
    pub(super) time_handle: driver::TimeHandle,

    /// Source of `Instant::now()`
    #[cfg_attr(not(all(feature = "time", feature = "test-util")), allow(dead_code))]
    pub(super) clock: driver::Clock,

    /// Blocking pool spawner
    pub(super) blocking_spawner: blocking::Spawner,
}

/// Runtime context guard.
///
/// Returned by [`Runtime::enter`] and [`Handle::enter`], the context guard exits
/// the runtime context on drop.
///
/// [`Runtime::enter`]: fn@crate::runtime::Runtime::enter
#[derive(Debug)]
#[must_use = "Creating and dropping a guard does nothing"]
pub struct EnterGuard<'a> {
    _guard: context::EnterGuard,
    _handle_lifetime: PhantomData<&'a Handle>,
}

impl Handle {
    /// Enters the runtime context. This allows you to construct types that must
    /// have an executor available on creation such as [`Sleep`] or [`TcpStream`].
    /// It will also allow you to call methods such as [`tokio::spawn`].
    ///
    /// [`Sleep`]: struct@crate::time::Sleep
    /// [`TcpStream`]: struct@crate::net::TcpStream
    /// [`tokio::spawn`]: fn@crate::spawn
    pub fn enter(&self) -> EnterGuard<'_> {
        EnterGuard {
            _guard: context::enter(self.clone()),
            _handle_lifetime: PhantomData,
        }
    }

    /// Returns a `Handle` view over the currently running `Runtime`.
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
    ///     handle.spawn(async { /* ... */ })
    ///     // This next line would cause a panic
    ///     // let handle2 = Handle::current();
    /// });
    /// # handle.join().unwrap();
    /// # });
    /// # }
    /// ```
    pub fn current() -> Self {
        context::current()
    }

    /// Returns a Handle view over the currently running Runtime
    ///
    /// Returns an error if no Runtime has been started
    ///
    /// Contrary to `current`, this never panics
    pub fn try_current() -> Result<Self, TryCurrentError> {
        context::try_current()
    }

    cfg_stats! {
        /// Returns a view that lets you get information about how the runtime
        /// is performing.
        pub fn stats(&self) -> &crate::runtime::stats::RuntimeStats {
            self.spawner.stats()
        }
    }

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
    /// // Get a handle from this runtime
    /// let handle = rt.handle();
    ///
    /// // Spawn a future onto the runtime using the handle
    /// handle.spawn(async {
    ///     println!("now running on a worker thread");
    /// });
    /// # }
    /// ```
    #[cfg_attr(tokio_track_caller, track_caller)]
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        let future = crate::util::trace::task(future, "task", None);
        self.spawner.spawn(future)
    }

    /// Runs the provided function on an executor dedicated to blocking.
    /// operations.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime::Runtime;
    ///
    /// # fn dox() {
    /// // Create the runtime
    /// let rt = Runtime::new().unwrap();
    /// // Get a handle from this runtime
    /// let handle = rt.handle();
    ///
    /// // Spawn a blocking function onto the runtime using the handle
    /// handle.spawn_blocking(|| {
    ///     println!("now running on a worker thread");
    /// });
    /// # }
    #[cfg_attr(tokio_track_caller, track_caller)]
    pub fn spawn_blocking<F, R>(&self, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        if cfg!(debug_assertions) && std::mem::size_of::<F>() > 2048 {
            self.spawn_blocking_inner(Box::new(func), None)
        } else {
            self.spawn_blocking_inner(func, None)
        }
    }

    #[cfg_attr(tokio_track_caller, track_caller)]
    pub(crate) fn spawn_blocking_inner<F, R>(&self, func: F, name: Option<&str>) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let fut = BlockingTask::new(func);

        #[cfg(all(tokio_unstable, feature = "tracing"))]
        let fut = {
            use tracing::Instrument;
            #[cfg(tokio_track_caller)]
            let location = std::panic::Location::caller();
            #[cfg(tokio_track_caller)]
            let span = tracing::trace_span!(
                target: "tokio::task::blocking",
                "runtime.spawn",
                kind = %"blocking",
                task.name = %name.unwrap_or_default(),
                "fn" = %std::any::type_name::<F>(),
                spawn.location = %format_args!("{}:{}:{}", location.file(), location.line(), location.column()),
            );
            #[cfg(not(tokio_track_caller))]
            let span = tracing::trace_span!(
                target: "tokio::task::blocking",
                "runtime.spawn",
                kind = %"blocking",
                task.name = %name.unwrap_or_default(),
                "fn" = %std::any::type_name::<F>(),
            );
            fut.instrument(span)
        };

        #[cfg(not(all(tokio_unstable, feature = "tracing")))]
        let _ = name;

        let (task, handle) = task::unowned(fut, NoopSchedule);
        let _ = self.blocking_spawner.spawn(task, self);
        handle
    }

    /// Runs a future to completion on this `Handle`'s associated `Runtime`.
    ///
    /// This runs the given future on the current thread, blocking until it is
    /// complete, and yielding its resolved result. Any tasks or timers which
    /// the future spawns internally will be executed on the runtime.
    ///
    /// When this is used on a `current_thread` runtime, only the
    /// [`Runtime::block_on`] method can drive the IO and timer drivers, but the
    /// `Handle::block_on` method cannot drive them. This means that, when using
    /// this method on a current_thread runtime, anything that relies on IO or
    /// timers will not work unless there is another thread currently calling
    /// [`Runtime::block_on`] on the same runtime.
    ///
    /// # If the runtime has been shut down
    ///
    /// If the `Handle`'s associated `Runtime` has been shut down (through
    /// [`Runtime::shutdown_background`], [`Runtime::shutdown_timeout`], or by
    /// dropping it) and `Handle::block_on` is used it might return an error or
    /// panic. Specifically IO resources will return an error and timers will
    /// panic. Runtime independent futures will run as normal.
    ///
    /// # Panics
    ///
    /// This function panics if the provided future panics, if called within an
    /// asynchronous execution context, or if a timer future is executed on a
    /// runtime that has been shut down.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime::Runtime;
    ///
    /// // Create the runtime
    /// let rt  = Runtime::new().unwrap();
    ///
    /// // Get a handle from this runtime
    /// let handle = rt.handle();
    ///
    /// // Execute the future, blocking the current thread until completion
    /// handle.block_on(async {
    ///     println!("hello");
    /// });
    /// ```
    ///
    /// Or using `Handle::current`:
    ///
    /// ```
    /// use tokio::runtime::Handle;
    ///
    /// #[tokio::main]
    /// async fn main () {
    ///     let handle = Handle::current();
    ///     std::thread::spawn(move || {
    ///         // Using Handle::block_on to run async code in the new thread.
    ///         handle.block_on(async {
    ///             println!("hello");
    ///         });
    ///     });
    /// }
    /// ```
    ///
    /// [`JoinError`]: struct@crate::task::JoinError
    /// [`JoinHandle`]: struct@crate::task::JoinHandle
    /// [`Runtime::block_on`]: fn@crate::runtime::Runtime::block_on
    /// [`Runtime::shutdown_background`]: fn@crate::runtime::Runtime::shutdown_background
    /// [`Runtime::shutdown_timeout`]: fn@crate::runtime::Runtime::shutdown_timeout
    /// [`spawn_blocking`]: crate::task::spawn_blocking
    /// [`tokio::fs`]: crate::fs
    /// [`tokio::net`]: crate::net
    /// [`tokio::time`]: crate::time
    #[cfg_attr(tokio_track_caller, track_caller)]
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        let future = crate::util::trace::task(future, "block_on", None);

        // Enter the **runtime** context. This configures spawning, the current I/O driver, ...
        let _rt_enter = self.enter();

        // Enter a **blocking** context. This prevents blocking from a runtime.
        let mut blocking_enter = crate::runtime::enter(true);

        // Block on the future
        blocking_enter
            .block_on(future)
            .expect("failed to park thread")
    }

    pub(crate) fn shutdown(mut self) {
        self.spawner.shutdown();
    }
}

/// Error returned by `try_current` when no Runtime has been started
#[derive(Debug)]
pub struct TryCurrentError {
    kind: TryCurrentErrorKind,
}

impl TryCurrentError {
    pub(crate) fn new_no_context() -> Self {
        Self {
            kind: TryCurrentErrorKind::NoContext,
        }
    }

    pub(crate) fn new_thread_local_destroyed() -> Self {
        Self {
            kind: TryCurrentErrorKind::ThreadLocalDestroyed,
        }
    }

    /// Returns true if the call failed because there is currently no runtime in
    /// the Tokio context.
    pub fn is_missing_context(&self) -> bool {
        matches!(self.kind, TryCurrentErrorKind::NoContext)
    }

    /// Returns true if the call failed because the Tokio context thread-local
    /// had been destroyed. This can usually only happen if in the destructor of
    /// other thread-locals.
    pub fn is_thread_local_destroyed(&self) -> bool {
        matches!(self.kind, TryCurrentErrorKind::ThreadLocalDestroyed)
    }
}

enum TryCurrentErrorKind {
    NoContext,
    ThreadLocalDestroyed,
}

impl fmt::Debug for TryCurrentErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TryCurrentErrorKind::*;
        match self {
            NoContext => f.write_str("NoContext"),
            ThreadLocalDestroyed => f.write_str("ThreadLocalDestroyed"),
        }
    }
}

impl fmt::Display for TryCurrentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TryCurrentErrorKind::*;
        match self.kind {
            NoContext => f.write_str(CONTEXT_MISSING_ERROR),
            ThreadLocalDestroyed => f.write_str(THREAD_LOCAL_DESTROYED_ERROR),
        }
    }
}

impl error::Error for TryCurrentError {}
