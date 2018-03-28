use builder::Builder;
use inner::Inner;
use sender::Sender;
use shutdown::Shutdown;

use futures::Future;

/// Work-stealing based thread pool for executing futures.
///
/// If a `ThreadPool` instance is dropped without explicitly being shutdown,
/// `shutdown_now` is called implicitly, forcing all tasks that have not yet
/// completed to be dropped.
///
/// Create `ThreadPool` instances using `Builder`.
#[derive(Debug)]
pub struct ThreadPool {
    pub(crate) inner: Option<Sender>,
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
    where F: Future<Item = (), Error = ()> + Send + 'static,
    {
        self.sender().spawn(future).unwrap();
    }

    /// Return a reference to the sender handle
    ///
    /// The handle is used to spawn futures onto the thread pool. It also
    /// implements the `Executor` trait.
    pub fn sender(&self) -> &Sender {
        self.inner.as_ref().unwrap()
    }

    /// Return a mutable reference to the sender handle
    pub fn sender_mut(&mut self) -> &mut Sender {
        self.inner.as_mut().unwrap()
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
        self.inner().shutdown(false, false);
        Shutdown { inner: self.inner.take().unwrap() }
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
        self.inner().shutdown(true, false);
        Shutdown { inner: self.inner.take().unwrap() }
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
        self.inner().shutdown(true, true);
        Shutdown { inner: self.inner.take().unwrap() }
    }

    fn inner(&self) -> &Inner {
        &*self.inner.as_ref().unwrap().inner
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        if let Some(sender) = self.inner.take() {
            sender.inner.shutdown(true, true);
            let shutdown = Shutdown { inner: sender };
            let _ = shutdown.wait();
        }
    }
}
