use tokio_executor::{threadpool::Sender, Executor};
use tokio_executor_01::{self as executor_01, Executor as Executor01};

use futures_01::future::Future as Future01;
use futures_util::{compat::Future01CompatExt, future::FutureExt};
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
    pub(super) inner: super::CompatSender<Sender>,
}

impl TaskExecutor {
    /// Spawn a `futures` 0.1 future onto the Tokio runtime.
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
    /// use tokio_compat::runtime::Runtime;
    /// # fn dox() {
    /// // Create the runtime
    /// let rt = Runtime::new().unwrap();
    /// let executor = rt.executor();
    ///
    /// // Spawn a future onto the runtime
    /// executor.spawn(futures_01::future::lazy(|| {
    ///     println!("now running on a worker thread");
    ///     Ok(())
    /// }));
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn<F>(&self, future: F)
    where
        F: Future01<Item = (), Error = ()> + Send + 'static,
    {
        let future = Box::pin(future.compat().map(|_| ()));
        self.inner.0.spawn(future).unwrap();
    }

    /// Spawn a `std::future` future onto the Tokio runtime.
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
    /// use tokio_compat::runtime::Runtime;
    ///
    /// # fn dox() {
    /// // Create the runtime
    /// let rt = Runtime::new().unwrap();
    /// let executor = rt.executor();
    ///
    /// // Spawn a future onto the runtime
    /// executor.spawn_std(async {
    ///     println!("now running on a worker thread");
    /// });
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn_std<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.inner.0.spawn(future).unwrap();
    }
}

impl Executor for TaskExecutor {
    fn spawn(
        &mut self,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Result<(), tokio_executor::SpawnError> {
        self.inner.0.spawn(future)
    }

    fn status(&self) -> Result<(), tokio_executor::SpawnError> {
        self.inner.0.status()
    }
}

impl<T> tokio_executor::TypedExecutor<T> for TaskExecutor
where
    T: Future<Output = ()> + Send + 'static,
{
    fn spawn(&mut self, future: T) -> Result<(), tokio_executor::SpawnError> {
        self.inner.0.spawn(Box::pin(future))
    }

    fn status(&self) -> Result<(), tokio_executor::SpawnError> {
        Executor::status(&self.inner.0)
    }
}

impl Executor01 for TaskExecutor {
    fn spawn(
        &mut self,
        future: Box<dyn Future01<Item = (), Error = ()> + Send>,
    ) -> Result<(), executor_01::SpawnError> {
        self.inner.spawn(future)
    }

    fn status(&self) -> Result<(), executor_01::SpawnError> {
        self.inner.status()
    }
}

impl<T> executor_01::TypedExecutor<T> for TaskExecutor
where
    T: Future01<Item = (), Error = ()> + Send + 'static,
{
    fn spawn(&mut self, future: T) -> Result<(), executor_01::SpawnError> {
        executor_01::TypedExecutor::spawn(&mut self.inner, future)
    }

    fn status(&self) -> Result<(), executor_01::SpawnError> {
        Executor01::status(&self.inner)
    }
}
