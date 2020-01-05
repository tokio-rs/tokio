use crate::loom::sync::Arc;
use crate::runtime::thread_pool::slice;
use crate::task::JoinHandle;

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
pub(crate) struct Spawner {
    workers: Arc<slice::Set>,
}

impl Spawner {
    pub(super) fn new(workers: Arc<slice::Set>) -> Spawner {
        Spawner { workers }
    }

    /// Spawn a future onto the thread pool
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.workers.spawn_typed(future)
    }

    /// Reference to the worker set. Used by `ThreadPool` to initiate shutdown.
    pub(super) fn workers(&self) -> &slice::Set {
        &*self.workers
    }
}

impl fmt::Debug for Spawner {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Spawner").finish()
    }
}
