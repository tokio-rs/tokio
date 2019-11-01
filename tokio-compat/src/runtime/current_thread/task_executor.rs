use futures_01::future::{self as future_01, Future as Future01};
use futures_util::{compat::Future01CompatExt, FutureExt};
use std::{future::Future, pin::Pin};
use tokio_02::executor::{self as executor_02, current_thread::TaskExecutor as TaskExecutor02};
use tokio_executor_01 as executor_01;

/// Executes futures on the current thread.
///
/// All futures executed using this executor will be executed on the current
/// thread. As such, `run` will wait for these futures to complete before
/// returning.
///
/// For more details, see the [module level](../index.html) documentation.
#[derive(Clone, Debug)]
pub struct TaskExecutor {
    inner: TaskExecutor02,
}

impl TaskExecutor {
    /// Returns an executor that executes futures on the current thread.
    ///
    /// The user of `TaskExecutor` must ensure that when a future is submitted,
    /// that it is done within the context of a call to `run`.
    ///
    /// For more details, see the [module level](index.html) documentation.
    pub fn current() -> TaskExecutor {
        TaskExecutor {
            inner: TaskExecutor02::current(),
        }
    }

    /// Spawn a `futures` 0.1 future onto the current `CurrentThread` instance.
    pub fn spawn_local(
        &mut self,
        future: impl Future01<Item = (), Error = ()> + 'static,
    ) -> Result<(), executor_01::SpawnError> {
        let future = Box::pin(future.compat().map(|_| ()));
        self.spawn_local_std(future).map_err(map_spawn_err)
    }

    /// Spawn a `std::future` future onto the current `CurrentThread` instance.
    pub fn spawn_local_std(
        &mut self,
        future: Pin<Box<dyn Future<Output = ()>>>,
    ) -> Result<(), executor_02::SpawnError> {
        self.inner.spawn_local(future)
    }
}

fn map_spawn_err(new: executor_02::SpawnError) -> executor_01::SpawnError {
    match new {
        _ if new.is_shutdown() => executor_01::SpawnError::shutdown(),
        _ if new.is_at_capacity() => executor_01::SpawnError::at_capacity(),
        e => unreachable!("weird spawn error {:?}", e),
    }
}

impl executor_01::Executor for TaskExecutor {
    fn spawn(
        &mut self,
        future: Box<dyn Future01<Item = (), Error = ()> + Send>,
    ) -> Result<(), executor_01::SpawnError> {
        self.spawn_local(future)
    }

    fn status(&self) -> Result<(), executor_01::SpawnError> {
        executor_02::Executor::status(&self.inner).map_err(map_spawn_err)
    }
}

impl<F> executor_01::TypedExecutor<F> for TaskExecutor
where
    F: Future01<Item = (), Error = ()> + 'static,
{
    fn spawn(&mut self, future: F) -> Result<(), executor_01::SpawnError> {
        let future = Box::pin(future.compat().map(|_| ()));
        self.spawn_local_std(future).map_err(map_spawn_err)
    }

    fn status(&self) -> Result<(), executor_01::SpawnError> {
        executor_02::Executor::status(&self.inner).map_err(map_spawn_err)
    }
}

impl executor_02::Executor for TaskExecutor {
    fn spawn(
        &mut self,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Result<(), executor_02::SpawnError> {
        self.spawn_local_std(future)
    }

    fn status(&self) -> Result<(), executor_02::SpawnError> {
        executor_02::Executor::status(&self.inner)
    }
}

impl<F> executor_02::TypedExecutor<F> for TaskExecutor
where
    F: Future<Output = ()> + 'static,
{
    fn spawn(&mut self, future: F) -> Result<(), executor_02::SpawnError> {
        self.spawn_local_std(Box::pin(future))
    }

    fn status(&self) -> Result<(), executor_02::SpawnError> {
        executor_02::Executor::status(&self.inner)
    }
}

impl<F> future_01::Executor<F> for TaskExecutor
where
    F: Future01<Item = (), Error = ()> + 'static,
{
    fn execute(&self, future: F) -> Result<(), future_01::ExecuteError<F>> {
        match executor_02::Executor::status(&self.inner) {
            Err(e) if e.is_shutdown() => Err(future_01::ExecuteError::new(
                future_01::ExecuteErrorKind::Shutdown,
                future,
            )),
            Err(e) if e.is_at_capacity() => Err(future_01::ExecuteError::new(
                future_01::ExecuteErrorKind::NoCapacity,
                future,
            )),
            Err(e) => panic!("unexpected spawn error {:?}", e),
            Ok(_) => {
                let mut this = self.clone();
                this.spawn_local(future).unwrap_or_else(|e| {
                    debug_assert!(false, "status succeeded, but spawn failed: {}", e)
                });
                Ok(())
            }
        }
    }
}
