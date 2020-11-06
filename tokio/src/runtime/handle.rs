use crate::future::poll_fn;
use crate::park::Park;
use crate::runtime::blocking::task::BlockingTask;
use crate::runtime::task::{self, JoinHandle};
use crate::runtime::{basic_scheduler, blocking, context, driver, Spawner};

use std::future::Future;
use std::task::Poll::{Pending, Ready};
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
    pub(super) io_handle: driver::IoHandle,

    /// Handles to the signal drivers
    pub(super) signal_handle: driver::SignalHandle,

    /// Handles to the time drivers
    pub(super) time_handle: driver::TimeHandle,

    /// Source of `Instant::now()`
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
pub struct EnterGuard<'a> {
    handle: &'a Handle,
    guard: context::EnterGuard,
}

impl Handle {
    /// Enter the runtime context. This allows you to construct types that must
    /// have an executor available on creation such as [`Sleep`] or [`TcpStream`].
    /// It will also allow you to call methods such as [`tokio::spawn`].
    ///
    /// [`Sleep`]: struct@crate::time::Sleep
    /// [`TcpStream`]: struct@crate::net::TcpStream
    /// [`tokio::spawn`]: fn@crate::spawn
    pub fn enter(&self) -> EnterGuard<'_> {
        EnterGuard {
            handle: self,
            guard: context::enter(self.clone()),
        }
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
    ///     handle.spawn(async { /* ... */ })
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

    /// Spawn a future onto the Tokio runtime.
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
        #[cfg(feature = "tracing")]
        let future = crate::util::trace::task(future, "task");
        self.spawner.spawn(future)
    }

    /// Run the provided function on an executor dedicated to blocking
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
        #[cfg(feature = "tracing")]
        let func = {
            #[cfg(tokio_track_caller)]
            let location = std::panic::Location::caller();
            #[cfg(tokio_track_caller)]
            let span = tracing::trace_span!(
                target: "tokio::task",
                "task",
                kind = %"blocking",
                function = %std::any::type_name::<F>(),
                spawn.location = %format_args!("{}:{}:{}", location.file(), location.line(), location.column()),
            );
            #[cfg(not(tokio_track_caller))]
            let span = tracing::trace_span!(
                target: "tokio::task",
                "task",
                kind = %"blocking",
                function = %std::any::type_name::<F>(),
            );
            move || {
                let _g = span.enter();
                func()
            }
        };
        let (task, handle) = task::joinable(BlockingTask::new(func));
        let _ = self.blocking_spawner.spawn(task, &self);
        handle
    }

    /// Run a future to completion on the Tokio runtime. This is the
    /// runtime's entry point.
    ///
    /// This runs the given future on the runtime, blocking until it is
    /// complete, and yielding its resolved result. Any tasks or timers
    /// which the future spawns internally will be executed on the runtime.
    ///
    /// # Multi thread scheduler
    ///
    /// When the multi thread scheduler is used this will allow futures
    /// to run within the io driver and timer context of the overall runtime.
    ///
    /// # Current thread scheduler
    ///
    /// When the current thread scheduler is enabled `block_on`
    /// can be called concurrently from multiple threads. The first call
    /// will take ownership of the io and timer drivers. This means
    /// other threads which do not own the drivers will hook into that one.
    /// When the first `block_on` completes, other threads will be able to
    /// "steal" the driver to allow continued execution of their futures.
    ///
    /// # Panics
    ///
    /// This function panics if the provided future panics, or if called within an
    /// asynchronous execution context.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::runtime::Runtime;
    ///
    /// // Create the runtime
    /// let rt  = Runtime::new().unwrap();
    /// // Get a handle to rhe runtime
    /// let handle = rt.handle();
    ///
    /// // Execute the future, blocking the current thread until completion using the handle
    /// handle.block_on(async {
    ///     println!("hello");
    /// });
    /// ```
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        match &self.spawner {
            Spawner::Basic(exec) => {
                self.block_on_with_basic_scheduler::<F, crate::park::thread::ParkThread>(future, exec, None)
            }
            #[cfg(feature = "rt-multi-thread")]
            Spawner::ThreadPool(_) => {
                let _enter = self.enter();
                let mut enter = crate::runtime::enter(true);
                enter.block_on(future).expect("failed to park thread")
            }
        }
    }

    pub(crate) fn block_on_with_basic_scheduler<F: Future, P: Park>(
        &self,
        future: F,
        spawner: &basic_scheduler::Spawner,
        scheduler: Option<&basic_scheduler::BasicScheduler<P>>,
    ) -> F::Output {
        let _enter = self.enter();

        pin!(future);

        // Attempt to steal the dedicated parker and block_on the future if we can there,
        // othwerwise, lets select on a notification that the parker is available
        // or the future is complete.
        loop {
            if let Some(mut inner) = scheduler.and_then(basic_scheduler::BasicScheduler::take_inner)
            {
                return inner.block_on(future);
            }

            let mut enter = crate::runtime::enter(false);

            let notified = spawner.notify().notified();
            pin!(notified);

            if let Some(out) = enter
                .block_on(poll_fn(|cx| {
                    if notified.as_mut().poll(cx).is_ready() {
                        return Ready(None);
                    }

                    if let Ready(out) = future.as_mut().poll(cx) {
                        return Ready(Some(out));
                    }

                    Pending
                }))
                .expect("Failed to `Enter::block_on`")
            {
                return out;
            }
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
