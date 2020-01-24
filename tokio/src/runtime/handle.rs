use crate::runtime::{blocking, context, io, time, Spawner};
use std::{error, fmt};

cfg_rt_core! {
    use crate::task::JoinHandle;

    use std::future::Future;
}

/// Handle to the runtime
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
    /// Enter the runtime context
    pub fn enter<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        context::enter(self.clone(), f)
    }

    /// Returns a Handle view over the currently running Runtime
    ///
    /// # Panic
    ///
    /// A Runtime must have been started or this will panic
    ///
    /// # Examples
    ///
    /// This allows for the current handle to be gotten when running in a `#`
    ///
    /// ```
    /// # use tokio::runtime::Runtime;
    ///
    /// # fn dox() {
    /// # let rt = Runtime::new().unwrap();
    /// # rt.spawn(async {
    /// use tokio::runtime::Handle;
    ///
    /// let handle = Handle::current();
    /// handle.spawn(async {
    ///     println!("now running in the existing Runtime");
    /// })
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
        /// This function panics if the spawn fails. Failure occurs if the executor
        /// is currently at capacity and is unable to spawn a new future.
        pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
        where
            F: Future + Send + 'static,
            F::Output: Send + 'static,
        {
            self.spawner.spawn(future)
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
