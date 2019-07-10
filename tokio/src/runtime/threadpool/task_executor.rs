use tokio_executor::SpawnError;
use tokio_threadpool::Sender;

use std::future::Future;
use std::pin::Pin;

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
    where F: Future<Output = ()> + Send + 'static,
    {
        self.inner.spawn(future).unwrap();
    }
}

impl tokio_executor::Executor for TaskExecutor {
    fn spawn(
        &mut self,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Result<(), SpawnError> {
        self.inner.spawn(future)
    }
}

impl<T> tokio_executor::TypedExecutor<T> for TaskExecutor
where
    T: Future<Output = ()> + Send + 'static,
{
    fn spawn(&mut self, future: T) -> Result<(), crate::executor::SpawnError> {
        crate::executor::Executor::spawn(self, Box::pin(future))
    }
}
