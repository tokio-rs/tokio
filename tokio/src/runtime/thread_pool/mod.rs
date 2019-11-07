//! Threadpool

mod current;

mod idle;
use self::idle::Idle;

mod owned;
use self::owned::Owned;

mod queue;

mod spawner;
pub(crate) use self::spawner::Spawner;

mod set;

mod shared;
use self::shared::Shared;

mod shutdown;

mod worker;
use self::worker::Worker;

#[cfg(feature = "blocking")]
pub(crate) use worker::blocking;

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

use crate::loom::sync::Arc;
use crate::runtime::blocking::{self, PoolWaiter};
use crate::runtime::task::JoinHandle;
use crate::runtime::Park;

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

// The Arc<Box<_>> is needed because loom doesn't support Arc<T> where T: !Sized
// loom doesn't support that because it requires CoerceUnsized, which is
// unstable
type Callback = Arc<Box<dyn Fn(usize, &mut dyn FnMut()) + Send + Sync>>;

impl ThreadPool {
    pub(crate) fn new<F, P>(
        pool_size: usize,
        blocking_pool: Arc<blocking::Pool>,
        around_worker: Callback,
        mut build_park: F,
    ) -> ThreadPool
    where
        F: FnMut(usize) -> P,
        P: Park + Send + 'static,
    {
        let (shutdown_tx, shutdown_rx) = shutdown::channel();

        let launch_worker = Arc::new(Box::new(move |worker: Worker<BoxedPark<P>>| {
            // NOTE: It might seem like the shutdown_tx that's moved into this Arc is never
            // dropped, and that shutdown_rx will therefore never see EOF, but that is not actually
            // the case. Only `build_with_park` and each worker hold onto a copy of this Arc.
            // `build_with_park` drops it immediately, and the workers drop theirs when their `run`
            // method returns (and their copy of the Arc are dropped). In fact, we don't actually
            // _need_ a copy of `shutdown_tx` for each worker thread; having them all hold onto
            // this Arc, which in turn holds the last `shutdown_tx` would have been sufficient.
            let shutdown_tx = shutdown_tx.clone();
            let around_worker = around_worker.clone();

            Box::new(move || {
                struct AbortOnPanic;

                impl Drop for AbortOnPanic {
                    fn drop(&mut self) {
                        if std::thread::panicking() {
                            eprintln!("[ERROR] unhandled panic in Tokio scheduler. This is a bug and should be reported.");
                            std::process::abort();
                        }
                    }
                }

                let _abort_on_panic = AbortOnPanic;

                let idx = worker.id();
                let mut f = Some(move || worker.run());
                around_worker(idx, &mut || {
                    (f.take()
                        .expect("around_thread callback called closure twice"))(
                    )
                });

                // Dropping the handle must happen __after__ the callback
                drop(shutdown_tx);
            }) as Box<dyn FnOnce() + Send + 'static>
        })
            as Box<dyn Fn(Worker<BoxedPark<P>>) -> Box<dyn FnOnce() + Send> + Send + Sync>);

        let (pool, workers) = worker::create_set::<_, BoxedPark<P>>(
            pool_size,
            |i| Box::new(BoxedPark::new(build_park(i))),
            Arc::clone(&launch_worker),
            blocking_pool.clone(),
        );

        // Spawn threads for each worker
        for worker in workers {
            crate::runtime::blocking::Pool::spawn(&blocking_pool, launch_worker(worker))
        }

        let spawner = Spawner::new(pool);
        let blocking = crate::runtime::blocking::PoolWaiter::from(blocking_pool);

        // ThreadPool::from_parts(spawner, shutdown_rx, blocking)
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
