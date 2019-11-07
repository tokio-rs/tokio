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

mod shutdown;

mod worker;

pub(crate) use worker::block_in_place;

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

use crate::blocking;
use crate::loom::sync::Arc;
use crate::runtime::Park;
use crate::task::JoinHandle;

use std::fmt;
use std::future::Future;

/// Work-stealing based thread pool for executing futures.
pub(crate) struct ThreadPool {
    spawner: Spawner,

    /// Shutdown waiter
    shutdown_rx: shutdown::Receiver,
}

// The Arc<Box<_>> is needed because loom doesn't support Arc<T> where T: !Sized
// loom doesn't support that because it requires CoerceUnsized, which is
// unstable
type Callback = Arc<Box<dyn Fn(usize, &mut dyn FnMut()) + Send + Sync>>;

impl ThreadPool {
    pub(crate) fn new<F, P>(
        pool_size: usize,
        blocking_pool: blocking::Spawner,
        around_worker: Callback,
        mut build_park: F,
    ) -> ThreadPool
    where
        F: FnMut(usize) -> P,
        P: Park + Send + 'static,
    {
        let (shutdown_tx, shutdown_rx) = shutdown::channel();

        let (pool, workers) = worker::create_set::<_, BoxedPark<P>>(
            pool_size,
            |i| BoxedPark::new(build_park(i)),
            blocking_pool.clone(),
            around_worker,
            shutdown_tx,
        );

        // Spawn threads for each worker
        for worker in workers {
            blocking_pool.spawn_background(|| worker.run());
        }

        let spawner = Spawner::new(pool);

        ThreadPool {
            spawner,
            shutdown_rx,
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
            enter.block_on(future)
        })
    }

    /// Shutdown the thread pool.
    pub(crate) fn shutdown_now(&mut self) {
        if self.spawner.workers().close() {
            self.shutdown_rx.wait();
        }
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

// TODO: delete?
pub(crate) struct BoxedPark<P> {
    inner: P,
}

impl<P> BoxedPark<P> {
    pub(crate) fn new(inner: P) -> Self {
        BoxedPark { inner }
    }
}

impl<P> Park for BoxedPark<P>
where
    P: Park,
{
    type Unpark = Box<dyn crate::runtime::park::Unpark>;
    type Error = P::Error;

    fn unpark(&self) -> Self::Unpark {
        Box::new(self.inner.unpark())
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.inner.park()
    }

    fn park_timeout(&mut self, duration: std::time::Duration) -> Result<(), Self::Error> {
        self.inner.park_timeout(duration)
    }
}
