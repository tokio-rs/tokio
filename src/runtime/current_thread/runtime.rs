use tokio_current_thread::{self as current_thread, CurrentThread};
use tokio_current_thread::Handle as ExecutorHandle;
use runtime::current_thread::Builder;

use tokio_reactor::{self, Reactor};
use tokio_timer::clock::{self, Clock};
use tokio_timer::timer::{self, Timer};
use tokio_executor;

use futures::{future, Future};

use std::fmt;
use std::error::Error;
use std::io;

/// Single-threaded runtime provides a way to start reactor
/// and executor on the current thread.
///
/// See [module level][mod] documentation for more details.
///
/// [mod]: index.html
#[derive(Debug)]
pub struct Runtime {
    reactor_handle: tokio_reactor::Handle,
    timer_handle: timer::Handle,
    clock: Clock,
    executor: CurrentThread<Timer<Reactor>>,
}

/// Handle to spawn a future on the corresponding `CurrentThread` runtime instance
#[derive(Debug, Clone)]
pub struct Handle(ExecutorHandle);

impl Handle {
    /// Spawn a future onto the `CurrentThread` runtime instance corresponding to this handle
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the `CurrentThread`
    /// instance of the `Handle` does not exist anymore.
    pub fn spawn<F>(&self, future: F) -> Result<(), tokio_executor::SpawnError>
    where F: Future<Item = (), Error = ()> + Send + 'static {
        self.0.spawn(future)
    }

    /// Provides a best effort **hint** to whether or not `spawn` will succeed.
    ///
    /// This function may return both false positives **and** false negatives.
    /// If `status` returns `Ok`, then a call to `spawn` will *probably*
    /// succeed, but may fail. If `status` returns `Err`, a call to `spawn` will
    /// *probably* fail, but may succeed.
    ///
    /// This allows a caller to avoid creating the task if the call to `spawn`
    /// has a high likelihood of failing.
    pub fn status(&self) -> Result<(), tokio_executor::SpawnError> {
        self.0.status()
    }
}

impl<T> future::Executor<T> for Handle
where T: Future<Item = (), Error = ()> + Send + 'static,
{
    fn execute(&self, future: T) -> Result<(), future::ExecuteError<T>> {
        if let Err(e) = self.status() {
            let kind = if e.is_at_capacity() {
                future::ExecuteErrorKind::NoCapacity
            } else {
                future::ExecuteErrorKind::Shutdown
            };

            return Err(future::ExecuteError::new(kind, future));
        }

        let _ = self.spawn(future);
        Ok(())
    }
}

/// Error returned by the `run` function.
#[derive(Debug)]
pub struct RunError {
    inner: current_thread::RunError,
}

impl fmt::Display for RunError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.inner)
    }
}

impl Error for RunError {
    fn description(&self) -> &str {
        self.inner.description()
    }
    fn cause(&self) -> Option<&Error> {
        self.inner.cause()
    }
}

impl Runtime {
    /// Returns a new runtime initialized with default configuration values.
    pub fn new() -> io::Result<Runtime> {
        Builder::new().build()
    }

    pub(super) fn new2(
        reactor_handle: tokio_reactor::Handle,
        timer_handle: timer::Handle,
        clock: Clock,
        executor: CurrentThread<Timer<Reactor>>) -> Runtime
    {
        Runtime {
            reactor_handle,
            timer_handle,
            clock,
            executor,
        }
    }

    /// Get a new handle to spawn futures on the single-threaded Tokio runtime
    ///
    /// Different to the runtime itself, the handle can be sent to different
    /// threads.
    pub fn handle(&self) -> Handle {
        Handle(self.executor.handle().clone())
    }

    /// Spawn a future onto the single-threaded Tokio runtime.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate tokio;
    /// # extern crate futures;
    /// # use futures::{future, Future, Stream};
    /// use tokio::runtime::current_thread::Runtime;
    ///
    /// # fn dox() {
    /// // Create the runtime
    /// let mut rt = Runtime::new().unwrap();
    ///
    /// // Spawn a future onto the runtime
    /// rt.spawn(future::lazy(|| {
    ///     println!("running on the runtime");
    ///     Ok(())
    /// }));
    /// # }
    /// # pub fn main() {}
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn<F>(&mut self, future: F) -> &mut Self
    where F: Future<Item = (), Error = ()> + 'static,
    {
        self.executor.spawn(future);
        self
    }

    /// Runs the provided future, blocking the current thread until the future
    /// completes.
    ///
    /// This function can be used to synchronously block the current thread
    /// until the provided `future` has resolved either successfully or with an
    /// error. The result of the future is then returned from this function
    /// call.
    ///
    /// Note that this function will **also** execute any spawned futures on the
    /// current thread, but will **not** block until these other spawned futures
    /// have completed. Once the function returns, any uncompleted futures
    /// remain pending in the `Runtime` instance. These futures will not run
    /// until `block_on` or `run` is called again.
    ///
    /// The caller is responsible for ensuring that other spawned futures
    /// complete execution by calling `block_on` or `run`.
    pub fn block_on<F>(&mut self, f: F) -> Result<F::Item, F::Error>
        where F: Future
    {
        self.enter(|executor| {
            // Run the provided future
            let ret = executor.block_on(f);
            ret.map_err(|e| e.into_inner().expect("unexpected execution error"))
        })
    }

    /// Run the executor to completion, blocking the thread until **all**
    /// spawned futures have completed.
    pub fn run(&mut self) -> Result<(), RunError> {
        self.enter(|executor| executor.run())
            .map_err(|e| RunError {
                inner: e,
            })
    }

    fn enter<F, R>(&mut self, f: F) -> R
    where F: FnOnce(&mut current_thread::Entered<Timer<Reactor>>) -> R
    {
        let Runtime {
            ref reactor_handle,
            ref timer_handle,
            ref clock,
            ref mut executor,
            ..
        } = *self;

        // Binds an executor to this thread
        let mut enter = tokio_executor::enter().expect("Multiple executors at once");

        // This will set the default handle and timer to use inside the closure
        // and run the future.
        tokio_reactor::with_default(&reactor_handle, &mut enter, |enter| {
            clock::with_default(clock, enter, |enter| {
                timer::with_default(&timer_handle, enter, |enter| {
                    // The TaskExecutor is a fake executor that looks into the
                    // current single-threaded executor when used. This is a trick,
                    // because we need two mutable references to the executor (one
                    // to run the provided future, another to install as the default
                    // one). We use the fake one here as the default one.
                    let mut default_executor = current_thread::TaskExecutor::current();
                    tokio_executor::with_default(&mut default_executor, enter, |enter| {
                        let mut executor = executor.enter(enter);
                        f(&mut executor)
                    })
                })
            })
        })
    }
}
