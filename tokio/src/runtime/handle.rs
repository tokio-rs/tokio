use crate::runtime::blocking::task::BlockingTask;
use crate::runtime::context::{self, EnterGuard};
use crate::runtime::task::{self, JoinHandle};
use crate::runtime::{blocking, driver, Spawner};

use std::future::Future;

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

#[derive(Debug)]
pub struct HandleEnterGuard(EnterGuard);

impl Handle {
    /// Enter the runtime context. This allows you to construct types that must
    /// have an executor available on creation such as [`Sleep`] or [`TcpStream`].
    /// It will also allow you to call methods such as [`tokio::spawn`].
    pub fn enter(&self) -> HandleEnterGuard {
        HandleEnterGuard(context::enter(self.clone()))
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
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.spawner.spawn(future)
    }

    /// Run the provided function on an executor dedicated to blocking operations.
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
    pub fn spawn_blocking<F, R>(&self, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
    {
        #[cfg(feature = "tracing")]
        let func = {
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
}
