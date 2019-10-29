use crate::executor::thread_pool;
use crate::runtime::JoinHandle;

use std::future::Future;

/// Spawns futures on the runtime
///
/// All futures spawned using this executor will be submitted to the associated
/// Runtime's executor. This executor is usually a thread pool.
///
/// For more details, see the [module level](index.html) documentation.
#[derive(Debug, Clone)]
pub struct Spawner {
    inner: thread_pool::Spawner,
}

impl Spawner {
    pub(super) fn new(inner: thread_pool::Spawner) -> Spawner {
        Spawner { inner }
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
    /// let spawner = rt.spawner();
    ///
    /// // Spawn a future onto the runtime
    /// spawner.spawn(async {
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
        F: Future<Output = ()> + Send + 'static,
    {
        self.inner.spawn(future)
    }
}
