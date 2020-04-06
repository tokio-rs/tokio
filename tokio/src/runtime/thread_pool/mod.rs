//! Threadpool

mod atomic_cell;
use atomic_cell::AtomicCell;

mod idle;
use self::idle::Idle;

mod worker;
pub(crate) use worker::Launch;

cfg_blocking! {
    pub(crate) use worker::block_in_place;
}

use crate::loom::sync::Arc;
use crate::runtime::task::{self, JoinHandle};
use crate::runtime::Parker;

use std::fmt;
use std::future::Future;

/// Work-stealing based thread pool for executing futures.
pub(crate) struct ThreadPool {
    spawner: Spawner,
}

/// Submit futures to the associated thread pool for execution.
///
/// A `Spawner` instance is a handle to a single thread pool that allows the owner
/// of the handle to spawn futures onto the thread pool.
///
/// The `Spawner` handle is *only* used for spawning new futures. It does not
/// impact the lifecycle of the thread pool in any way. The thread pool may
/// shutdown while there are outstanding `Spawner` instances.
///
/// `Spawner` instances are obtained by calling [`ThreadPool::spawner`].
///
/// [`ThreadPool::spawner`]: method@ThreadPool::spawner
#[derive(Clone)]
pub(crate) struct Spawner {
    shared: Arc<worker::Shared>,
}

// ===== impl ThreadPool =====

impl ThreadPool {
    pub(crate) fn new(size: usize, parker: Parker) -> (ThreadPool, Launch) {
        let (shared, launch) = worker::create(size, parker);
        let spawner = Spawner { shared };
        let thread_pool = ThreadPool { spawner };

        (thread_pool, launch)
    }

    /// Returns reference to `Spawner`.
    ///
    /// The `Spawner` handle can be cloned and enables spawning tasks from other
    /// threads.
    pub(crate) fn spawner(&self) -> &Spawner {
        &self.spawner
    }

    /// Spawns a task
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.spawner.spawn(future)
    }

    /// Blocks the current thread waiting for the future to complete.
    ///
    /// The future will execute on the current thread, but all spawned tasks
    /// will be executed on the thread pool.
    pub(crate) fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        let mut enter = crate::runtime::enter();
        enter.block_on(future).expect("failed to park thread")
    }
}

impl fmt::Debug for ThreadPool {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("ThreadPool").finish()
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.spawner.shared.close();
    }
}

// ==== impl Spawner =====

impl Spawner {
    /// Spawns a future onto the thread pool
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (task, handle) = task::joinable(future);
        self.shared.schedule(task, false);
        handle
    }
}

impl fmt::Debug for Spawner {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Spawner").finish()
    }
}
