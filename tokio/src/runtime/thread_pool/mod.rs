//! Threadpool

mod current;

mod idle;
use self::idle::Idle;

mod owned;
use self::owned::Owned;

mod queue;

mod spawner;
pub(crate) use self::spawner::Spawner;

mod slice;

mod shared;
use self::shared::Shared;

mod worker;
use worker::Worker;

cfg_blocking! {
    pub(crate) use worker::block_in_place;
}

/// Unit tests
#[cfg(test)]
mod tests;

#[cfg(not(loom))]
const LOCAL_QUEUE_CAPACITY: usize = 256;

// Shrink the size of the local queue when using loom. This shouldn't impact
// logic, but allows loom to test more edge cases in a reasonable a mount of
// time.
#[cfg(loom)]
const LOCAL_QUEUE_CAPACITY: usize = 2;

use crate::runtime::{self, blocking, Parker};
use crate::task::JoinHandle;

use std::fmt;
use std::future::Future;

/// Work-stealing based thread pool for executing futures.
pub(crate) struct ThreadPool {
    spawner: Spawner,
}

pub(crate) struct Workers {
    workers: Vec<Worker>,
}

impl ThreadPool {
    pub(crate) fn new(pool_size: usize, parker: Parker) -> (ThreadPool, Workers) {
        let (pool, workers) = worker::create_set(pool_size, parker);

        let spawner = Spawner::new(pool);

        let pool = ThreadPool { spawner };

        (pool, Workers { workers })
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
        self.spawner.enter(|| {
            let mut enter = crate::runtime::enter();
            enter.block_on(future)
        })
    }
}

impl fmt::Debug for ThreadPool {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("ThreadPool").finish()
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.spawner.workers().close();
    }
}

impl Workers {
    pub(crate) fn spawn(self, blocking_pool: &blocking::Spawner) {
        blocking_pool.enter(|| {
            for worker in self.workers {
                let b = blocking_pool.clone();
                runtime::spawn_blocking(move || worker.run(b));
            }
        });
    }
}
