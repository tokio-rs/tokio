use crate::runtime::{blocking, context, io, time, Spawner};

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
        let _e = context::ThreadContext::new(
            self.spawner.clone(),
            self.io_handle.clone(),
            self.time_handle.clone(),
            Some(self.clock.clone()),
            Some(self.blocking_spawner.clone()),
        )
        .enter();

        f()
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
        use crate::runtime::context::ThreadContext;

        Handle {
            spawner: ThreadContext::spawn_handle()
                .expect("not currently running on the Tokio runtime."),
            io_handle: ThreadContext::io_handle(),
            time_handle: ThreadContext::time_handle(),
            clock: ThreadContext::clock().expect("not currently running on the Tokio runtime."),
            blocking_spawner: ThreadContext::blocking_spawner()
                .expect("not currently running on the Tokio runtime."),
        }
    }
}

cfg_rt_core! {
    impl Handle {
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
