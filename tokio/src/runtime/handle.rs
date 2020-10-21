use crate::runtime::blocking::task::BlockingTask;
use crate::runtime::task::{self, JoinHandle};
use crate::runtime::{blocking, driver, Spawner};

/// Handle to the runtime.
///
/// The handle is internally reference-counted and can be freely cloned. A handle can be
/// obtained using the [`Runtime::handle`] method.
///
/// [`Runtime::handle`]: crate::runtime::Runtime::handle()
#[derive(Debug, Clone)]
pub(crate) struct Handle {
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

impl Handle {
    // /// Enter the runtime context. This allows you to construct types that must
    // /// have an executor available on creation such as [`Sleep`] or [`TcpStream`].
    // /// It will also allow you to call methods such as [`tokio::spawn`].
    // pub(crate) fn enter<F, R>(&self, f: F) -> R
    // where
    //     F: FnOnce() -> R,
    // {
    //     context::enter(self.clone(), f)
    // }

    /// Run the provided function on an executor dedicated to blocking operations.
    pub(crate) fn spawn_blocking<F, R>(&self, func: F) -> JoinHandle<R>
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
