use builder::Builder;
use pool::Pool;
use sender::Sender;
use shutdown::{Shutdown, ShutdownTrigger};

use futures::sync::oneshot;
use futures::{Future, Poll};

use std::sync::Arc;

/// Work-stealing based thread pool for executing futures.
///
/// If a `ThreadPool` instance is dropped without explicitly being shutdown,
/// `shutdown_now` is called implicitly, forcing all tasks that have not yet
/// completed to be dropped.
///
/// Create `ThreadPool` instances using `Builder`.
#[derive(Debug)]
pub struct ThreadPool {
    inner: Option<Inner>,
}

#[derive(Debug)]
struct Inner {
    sender: Sender,
    trigger: Arc<ShutdownTrigger>,
}

impl ThreadPool {
    /// Create a new `ThreadPool` with default values.
    ///
    /// Use [`Builder`] for creating a configured thread pool.
    ///
    /// [`Builder`]: struct.Builder.html
    pub fn new() -> ThreadPool {
        Builder::new().build()
    }

    pub(crate) fn new2(pool: Arc<Pool>, trigger: Arc<ShutdownTrigger>) -> ThreadPool {
        ThreadPool {
            inner: Some(Inner {
                sender: Sender { pool },
                trigger,
            }),
        }
    }

    /// Spawn a future onto the thread pool.
    ///
    /// This function takes ownership of the future and randomly assigns it to a
    /// worker thread. The thread will then start executing the future.
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
    /// thread_pool.spawn(lazy(|| {
    ///     println!("called from a worker thread");
    ///     Ok(())
    /// }));
    ///
    /// // Gracefully shutdown the threadpool
    /// thread_pool.shutdown().wait().unwrap();
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Use [`Sender::spawn`] for a
    /// version that returns a `Result` instead of panicking.
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Item = (), Error = ()> + Send + 'static,
    {
        self.sender().spawn(future).unwrap();
    }

    /// Spawn a future on to the thread pool, return a future representing
    /// the produced value.
    ///
    /// The SpawnHandle returned is a future that is a proxy for future itself.
    /// When future completes on this thread pool then the SpawnHandle will itself
    /// be resolved.
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
    /// let handle = thread_pool.spawn_handle(lazy(|| Ok::<_, ()>(42)));
    ///
    /// let value = handle.wait().unwrap();
    /// assert_eq!(value, 42);
    ///
    /// // Gracefully shutdown the threadpool
    /// thread_pool.shutdown().wait().unwrap();
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails.
    pub fn spawn_handle<F>(&self, future: F) -> SpawnHandle<F::Item, F::Error>
    where
        F: Future + Send + 'static,
        F::Item: Send + 'static,
        F::Error: Send + 'static,
    {
        SpawnHandle(oneshot::spawn(future, self.sender()))
    }

    /// Return a reference to the sender handle
    ///
    /// The handle is used to spawn futures onto the thread pool. It also
    /// implements the `Executor` trait.
    pub fn sender(&self) -> &Sender {
        &self.inner.as_ref().unwrap().sender
    }

    /// Return a mutable reference to the sender handle
    pub fn sender_mut(&mut self) -> &mut Sender {
        &mut self.inner.as_mut().unwrap().sender
    }

    /// Shutdown the pool once it becomes idle.
    ///
    /// Idle is defined as the completion of all futures that have been spawned
    /// onto the thread pool. There may still be outstanding handles when the
    /// thread pool reaches an idle state.
    ///
    /// Once the idle state is reached, calling `spawn` on any outstanding
    /// handle will result in an error. All worker threads are signaled and will
    /// shutdown. The returned future completes once all worker threads have
    /// completed the shutdown process.
    pub fn shutdown_on_idle(mut self) -> Shutdown {
        let inner = self.inner.take().unwrap();
        inner.sender.pool.shutdown(false, false);
        Shutdown::new(&inner.trigger)
    }

    /// Shutdown the pool
    ///
    /// This prevents the thread pool from accepting new tasks but will allow
    /// any existing tasks to complete.
    ///
    /// Calling `spawn` on any outstanding handle will result in an error. All
    /// worker threads are signaled and will shutdown. The returned future
    /// completes once all worker threads have completed the shutdown process.
    pub fn shutdown(mut self) -> Shutdown {
        let inner = self.inner.take().unwrap();
        inner.sender.pool.shutdown(true, false);
        Shutdown::new(&inner.trigger)
    }

    /// Shutdown the pool immediately
    ///
    /// This will prevent the thread pool from accepting new tasks **and**
    /// abort any tasks that are currently running on the thread pool.
    ///
    /// Calling `spawn` on any outstanding handle will result in an error. All
    /// worker threads are signaled and will shutdown. The returned future
    /// completes once all worker threads have completed the shutdown process.
    pub fn shutdown_now(mut self) -> Shutdown {
        let inner = self.inner.take().unwrap();
        inner.sender.pool.shutdown(true, true);
        Shutdown::new(&inner.trigger)
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            // Begin the shutdown process.
            inner.sender.pool.shutdown(true, true);
            let shutdown = Shutdown::new(&inner.trigger);

            // Drop `inner` in order to drop its shutdown trigger.
            drop(inner);

            // Wait until all worker threads terminate and the threadpool's resources clean up.
            let _ = shutdown.wait();
        }
    }
}

/// Handle returned from ThreadPool::spawn_handle.
///
/// This handle is a future representing the completion of a different future
/// spawned on to the thread pool. Created through the ThreadPool::spawn_handle
/// function this handle will resolve when the future provided resolves on the
/// thread pool.
#[derive(Debug)]
pub struct SpawnHandle<T, E>(oneshot::SpawnHandle<T, E>);

impl<T, E> Future for SpawnHandle<T, E> {
    type Item = T;
    type Error = E;

    fn poll(&mut self) -> Poll<T, E> {
        self.0.poll()
    }
}
