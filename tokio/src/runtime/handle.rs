use crate::runtime::{blocking, io, time};

cfg_rt_core! {
    use crate::runtime::basic_scheduler;
    use crate::task::JoinHandle;

    use std::future::Future;
}

cfg_rt_threaded! {
    use crate::runtime::thread_pool;
}

/// Handle to the runtime
#[derive(Debug, Clone)]
pub struct Handle {
    pub(super) kind: Kind,

    /// Handles to the I/O drivers
    pub(super) io_handles: Vec<io::Handle>,

    /// Handles to the time drivers
    pub(super) time_handles: Vec<time::Handle>,

    pub(super) clock: time::Clock,

    /// Blocking pool spawner
    pub(super) blocking_spawner: blocking::Spawner,
}

#[derive(Debug, Clone)]
pub(super) enum Kind {
    Shell,
    #[cfg(feature = "rt-core")]
    Basic(basic_scheduler::Spawner),
    #[cfg(feature = "rt-threaded")]
    ThreadPool(thread_pool::Spawner),
}

impl Handle {
    /// Enter the runtime context
    pub fn enter<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.blocking_spawner.enter(|| {
            let _io = io::set_default(&self.io_handles[0]);

            time::with_default(&self.time_handles[0], &self.clock, || match &self.kind {
                Kind::Shell => f(),
                #[cfg(feature = "rt-core")]
                Kind::Basic(spawner) => spawner.enter(f),
                #[cfg(feature = "rt-threaded")]
                Kind::ThreadPool(spawner) => spawner.enter(f),
            })
        })
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
            match &self.kind {
                Kind::Shell => panic!("spawning not enabled for runtime"),
                #[cfg(feature = "rt-core")]
                Kind::Basic(spawner) => spawner.spawn(future),
                #[cfg(feature = "rt-threaded")]
                Kind::ThreadPool(spawner) => spawner.spawn(future),
            }
        }
    }
}
