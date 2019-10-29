use crate::executor::loom::sync::Arc;
use crate::executor::park::Unpark;
use crate::executor::thread_pool::{worker, JoinHandle};

use std::fmt;
use std::future::Future;

/// Submit futures to the associated thread pool for execution.
///
/// A `Spawner` instance is a handle to a single thread pool, allowing the owner
/// of the handle to spawn futures onto the thread pool.
///
/// The `Spawner` handle is *only* used for spawning new futures. It does not
/// impact the lifecycle of the thread pool in any way. The thread pool may
/// shutdown while there are outstanding `Spawner` instances.
///
/// `Spawner` instances are obtained by calling [`ThreadPool::spawner`].
///
/// [`ThreadPool::spawner`]: struct.ThreadPool.html#method.spawner
#[derive(Clone)]
pub struct Spawner {
    workers: Arc<worker::Set<Box<dyn Unpark>>>,
}

impl Spawner {
    pub(super) fn new(workers: Arc<worker::Set<Box<dyn Unpark>>>) -> Spawner {
        Spawner { workers }
    }

    /// Spawn a future onto the thread pool
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.workers.spawn_typed(future)
    }

    /// Spawn a task in the background
    pub(super) fn spawn_background<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.workers.spawn_background(future);
    }

    pub(super) fn blocking_pool(&self) -> &Arc<crate::executor::blocking::Pool> {
        self.workers.blocking_pool()
    }

    /// Reference to the worker set. Used by `ThreadPool` to initiate shutdown.
    pub(super) fn workers(&self) -> &worker::Set<Box<dyn Unpark>> {
        &*self.workers
    }
}

impl fmt::Debug for Spawner {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Spawner").finish()
    }
}
