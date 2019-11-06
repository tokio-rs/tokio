use crate::runtime::blocking::PoolWaiter;
use crate::runtime::task::JoinHandle;
use crate::runtime::thread_pool::{shutdown, Spawner};

use std::fmt;
use std::future::Future;

/// Work-stealing based thread pool for executing futures.
pub(crate) struct ThreadPool {
    spawner: Spawner,

    /// Shutdown waiter
    shutdown_rx: shutdown::Receiver,

    /// Shutdown valve for Pool
    blocking: PoolWaiter,
}

impl ThreadPool {
    pub(super) fn from_parts(
        spawner: Spawner,
        shutdown_rx: shutdown::Receiver,
        blocking: PoolWaiter,
    ) -> ThreadPool {
        ThreadPool {
            spawner,
            shutdown_rx,
            blocking,
        }
    }

    /// Returns reference to `Spawner`.
    ///
    /// The `Spawner` handle can be cloned and enables spawning tasks from other
    /// threads.
    pub(crate) fn spawner(&self) -> &Spawner {
        &self.spawner
    }

    /// Spawn a task
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.spawner.spawn(future)
    }

    /// Block the current thread waiting for the future to complete.
    ///
    /// The future will execute on the current thread, but all spawned tasks
    /// will be executed on the thread pool.
    pub(crate) fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        crate::runtime::global::with_thread_pool(self.spawner(), || {
            let mut enter = crate::runtime::enter();
            crate::runtime::blocking::with_pool(self.spawner.blocking_pool(), || {
                enter.block_on(future)
            })
        })
    }

    /// Shutdown the thread pool.
    pub(crate) fn shutdown_now(&mut self) {
        if self.spawner.workers().close() {
            self.shutdown_rx.wait();
        }
        self.blocking.shutdown();
    }
}

impl fmt::Debug for ThreadPool {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("ThreadPool").finish()
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.shutdown_now();
    }
}
