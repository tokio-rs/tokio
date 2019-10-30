use crate::executor::blocking::PoolWaiter;
use crate::executor::thread_pool::{shutdown, Builder, JoinHandle, Spawner};
use crate::executor::Executor;

use std::fmt;
use std::future::Future;

/// Work-stealing based thread pool for executing futures.
pub struct ThreadPool {
    spawner: Spawner,

    /// Shutdown waiter
    shutdown_rx: shutdown::Receiver,

    /// Shutdown valve for Pool
    blocking: PoolWaiter,
}

impl ThreadPool {
    /// Create a new ThreadPool with default configuration
    pub fn new() -> ThreadPool {
        Builder::new().build()
    }

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
    pub fn spawner(&self) -> &Spawner {
        &self.spawner
    }

    /// Spawn a task
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.spawner.spawn(future)
    }

    /// Spawn a task in the background
    pub(crate) fn spawn_background<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawner.spawn_background(future);
    }

    /// Block the current thread waiting for the future to complete.
    ///
    /// The future will execute on the current thread, but all spawned tasks
    /// will be executed on the thread pool.
    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        crate::executor::global::with_threadpool(self, || {
            let mut enter =
                crate::executor::enter().expect("attempting to block while on a Tokio executor");
            crate::executor::blocking::with_pool(self.spawner.blocking_pool(), || {
                enter.block_on(future)
            })
        })
    }

    /// Shutdown the thread pool.
    pub fn shutdown_now(&mut self) {
        if self.spawner.workers().close() {
            self.shutdown_rx.wait();
        }
        self.blocking.shutdown();
    }
}

impl Default for ThreadPool {
    fn default() -> ThreadPool {
        ThreadPool::new()
    }
}

impl Executor for &ThreadPool {
    fn spawn(
        &mut self,
        future: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Result<(), crate::executor::SpawnError> {
        ThreadPool::spawn_background(self, future);
        Ok(())
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
