use crate::executor::current_thread;
#[cfg(feature = "rt-full")]
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
    kind: Kind
}

#[derive(Debug, Clone)]
enum Kind {
    #[cfg(feature = "rt-full")]
    ThreadPool(thread_pool::Spawner),
    CurrentThread(current_thread::Spawner),
}

impl Spawner {
    #[cfg(feature = "rt-full")]
    pub(super) fn thread_pool(spawner: thread_pool::Spawner) -> Spawner {
        Spawner { kind: Kind::ThreadPool(spawner) }
    }

    pub(super) fn current_thread(spawner: current_thread::Spawner) -> Spawner {
        Spawner { kind: Kind::CurrentThread(spawner) }
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
        match &self.kind {
            #[cfg(feature = "rt-full")]
            Kind::ThreadPool(spawner) => spawner.spawn(future),
            Kind::CurrentThread(spawner) => spawner.spawn(future),
        }
    }
}
