
use tokio_threadpool::Sender;

use futures::future::{self, Future};

/// Executes futures on the runtime
///
/// All futures spawned using this executor will be submitted to the associated
/// Runtime's executor. This executor is usually a thread pool.
///
/// For more details, see the [module level](index.html) documentation.
#[derive(Debug, Clone)]
pub struct TaskExecutor {
    pub(super) inner: Sender,
}

impl TaskExecutor {
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
    /// ```rust
    /// # extern crate tokio;
    /// # extern crate futures;
    /// # use futures::{future, Future, Stream};
    /// use tokio::runtime::Runtime;
    ///
    /// # fn dox() {
    /// // Create the runtime
    /// let mut rt = Runtime::new().unwrap();
    /// let executor = rt.executor();
    ///
    /// // Spawn a future onto the runtime
    /// executor.spawn(future::lazy(|| {
    ///     println!("now running on a worker thread");
    ///     Ok(())
    /// }));
    /// # }
    /// # pub fn main() {}
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn<F>(&self, future: F)
    where F: Future<Item = (), Error = ()> + Send + 'static,
    {
        self.inner.spawn(future).unwrap();
    }
}

impl<T> future::Executor<T> for TaskExecutor
where T: Future<Item = (), Error = ()> + Send + 'static,
{
    fn execute(&self, future: T) -> Result<(), future::ExecuteError<T>> {
        self.inner.execute(future)
    }
}

impl ::executor::Executor for TaskExecutor {
    fn spawn(&mut self, future: Box<Future<Item = (), Error = ()> + Send>)
        -> Result<(), ::executor::SpawnError>
    {
        self.inner.spawn(future)
    }
}
