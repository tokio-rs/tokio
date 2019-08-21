use pool::{self, Lifecycle, Pool, MAX_FUTURES};
use task::Task;

use std::sync::atomic::Ordering::{AcqRel, Acquire};
use std::sync::Arc;

use futures::{future, Future};
use tokio_executor::{self, SpawnError};

/// Submit futures to the associated thread pool for execution.
///
/// A `Sender` instance is a handle to a single thread pool, allowing the owner
/// of the handle to spawn futures onto the thread pool. New futures are spawned
/// using [`Sender::spawn`].
///
/// The `Sender` handle is *only* used for spawning new futures. It does not
/// impact the lifecycle of the thread pool in any way.
///
/// `Sender` instances are obtained by calling [`ThreadPool::sender`]. The
/// `Sender` struct implements the `Executor` trait.
///
/// [`Sender::spawn`]: #method.spawn
/// [`ThreadPool::sender`]: struct.ThreadPool.html#method.sender
#[derive(Debug)]
pub struct Sender {
    pub(crate) pool: Arc<Pool>,
}

impl Sender {
    /// Spawn a future onto the thread pool
    ///
    /// This function takes ownership of the future and spawns it onto the
    /// thread pool, assigning it to a worker thread. The exact strategy used to
    /// assign a future to a worker depends on if the caller is already on a
    /// worker thread or external to the thread pool.
    ///
    /// If the caller is currently on the thread pool, the spawned future will
    /// be assigned to the same worker that the caller is on. If the caller is
    /// external to the thread pool, the future will be assigned to a random
    /// worker.
    ///
    /// If `spawn` returns `Ok`, this does not mean that the future will be
    /// executed. The thread pool can be forcibly shutdown between the time
    /// `spawn` is called and the future has a chance to execute.
    ///
    /// If `spawn` returns `Err`, then the future failed to be spawned. There
    /// are two possible causes:
    ///
    /// * The thread pool is at capacity and is unable to spawn a new future.
    ///   This is a temporary failure. At some point in the future, the thread
    ///   pool might be able to spawn new futures.
    /// * The thread pool is shutdown. This is a permanent failure indicating
    ///   that the handle will never be able to spawn new futures.
    ///
    /// The status of the thread pool can be queried before calling `spawn`
    /// using the `status` function (part of the `Executor` trait).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::ThreadPool;
    /// use futures::future::{Future, lazy};
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = ThreadPool::new();
    ///
    /// thread_pool.sender().spawn(lazy(|| {
    ///     println!("called from a worker thread");
    ///     Ok(())
    /// })).unwrap();
    ///
    /// // Gracefully shutdown the threadpool
    /// thread_pool.shutdown().wait().unwrap();
    /// # }
    /// ```
    pub fn spawn<F>(&self, future: F) -> Result<(), SpawnError>
    where
        F: Future<Item = (), Error = ()> + Send + 'static,
    {
        let mut s = self;
        tokio_executor::Executor::spawn(&mut s, Box::new(future))
    }

    /// Logic to prepare for spawning
    fn prepare_for_spawn(&self) -> Result<(), SpawnError> {
        let mut state: pool::State = self.pool.state.load(Acquire).into();

        // Increment the number of futures spawned on the pool as well as
        // validate that the pool is still running/
        loop {
            let mut next = state;

            if next.num_futures() == MAX_FUTURES {
                // No capacity
                return Err(SpawnError::at_capacity());
            }

            if next.lifecycle() == Lifecycle::ShutdownNow {
                // Cannot execute the future, executor is shutdown.
                return Err(SpawnError::shutdown());
            }

            next.inc_num_futures();

            let actual = self
                .pool
                .state
                .compare_and_swap(state.into(), next.into(), AcqRel)
                .into();

            if actual == state {
                trace!("execute; count={:?}", next.num_futures());
                break;
            }

            state = actual;
        }

        Ok(())
    }
}

impl tokio_executor::Executor for Sender {
    fn status(&self) -> Result<(), tokio_executor::SpawnError> {
        let s = self;
        tokio_executor::Executor::status(&s)
    }

    fn spawn(
        &mut self,
        future: Box<dyn Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), SpawnError> {
        let mut s = &*self;
        tokio_executor::Executor::spawn(&mut s, future)
    }
}

impl<'a> tokio_executor::Executor for &'a Sender {
    fn status(&self) -> Result<(), tokio_executor::SpawnError> {
        let state: pool::State = self.pool.state.load(Acquire).into();

        if state.num_futures() == MAX_FUTURES {
            // No capacity
            return Err(SpawnError::at_capacity());
        }

        if state.lifecycle() == Lifecycle::ShutdownNow {
            // Cannot execute the future, executor is shutdown.
            return Err(SpawnError::shutdown());
        }

        Ok(())
    }

    fn spawn(
        &mut self,
        future: Box<dyn Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), SpawnError> {
        self.prepare_for_spawn()?;

        // At this point, the pool has accepted the future, so schedule it for
        // execution.

        // Create a new task for the future
        let task = Arc::new(Task::new(future));

        // Call `submit_external()` in order to place the task into the global
        // queue. This way all workers have equal chance of running this task,
        // which means IO handles will be assigned to reactors more evenly.
        self.pool.submit_external(task, &self.pool);

        Ok(())
    }
}

impl<T> tokio_executor::TypedExecutor<T> for Sender
where
    T: Future<Item = (), Error = ()> + Send + 'static,
{
    fn status(&self) -> Result<(), tokio_executor::SpawnError> {
        tokio_executor::Executor::status(self)
    }

    fn spawn(&mut self, future: T) -> Result<(), SpawnError> {
        tokio_executor::Executor::spawn(self, Box::new(future))
    }
}

impl<T> future::Executor<T> for Sender
where
    T: Future<Item = (), Error = ()> + Send + 'static,
{
    fn execute(&self, future: T) -> Result<(), future::ExecuteError<T>> {
        if let Err(e) = tokio_executor::Executor::status(self) {
            let kind = if e.is_at_capacity() {
                future::ExecuteErrorKind::NoCapacity
            } else {
                future::ExecuteErrorKind::Shutdown
            };

            return Err(future::ExecuteError::new(kind, future));
        }

        let _ = self.spawn(future);
        Ok(())
    }
}

impl Clone for Sender {
    #[inline]
    fn clone(&self) -> Sender {
        let pool = self.pool.clone();
        Sender { pool }
    }
}
