use crate::runtime::{blocking, context, io, time, Spawner};

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
    pub(crate) fn enter<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        context::enter(self.clone(), f)
    }
}
