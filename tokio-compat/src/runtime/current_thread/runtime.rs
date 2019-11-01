use super::{compat, Builder};

use tokio_02::executor::current_thread::Handle as ExecutorHandle;
use tokio_02::executor::current_thread::{self, CurrentThread};
use tokio_02::net::driver::{self, Reactor};
use tokio_02::timer::clock::{self, Clock};
use tokio_02::timer::timer::{self, Timer};
use tokio_executor_01 as executor_01;
use tokio_reactor_01 as reactor_01;
use tokio_timer_02 as timer_02;

use futures_01::future::Future as Future01;
use futures_util::{compat::Future01CompatExt, future::FutureExt};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::io;

/// Single-threaded runtime provides a way to start reactor
/// and executor on the current thread.
///
/// See [module level][mod] documentation for more details.
///
/// [mod]: index.html
#[derive(Debug)]
pub struct Runtime {
    reactor_handle: driver::Handle,
    timer_handle: timer::Handle,
    clock: Clock,
    executor: CurrentThread<Parker>,

    /// Compatibility background thread.
    ///
    /// This maintains a `tokio` 0.1 timer and reactor to support running
    /// futures that use older tokio APIs.
    compat: compat::Background,
}

pub(super) type Parker = Timer<Reactor>;

/// Handle to spawn a future on the corresponding `CurrentThread` runtime instance
#[derive(Debug, Clone)]
pub struct Handle(ExecutorHandle);

impl Handle {
    /// Spawn a `futures` 0.1 future onto the `CurrentThread` runtime instance
    /// corresponding to this handle.
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the `CurrentThread`
    /// instance of the `Handle` does not exist anymore.
    pub fn spawn<F>(&self, future: F) -> Result<(), executor_01::SpawnError>
    where
        F: Future01<Item = (), Error = ()> + Send + 'static,
    {
        self.0
            .spawn(future.compat().map(|_| ()))
            .map_err(compat::spawn_err)
    }

    /// Spawn a `std::future` future onto the `CurrentThread` runtime instance
    /// corresponding to this handle.
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the `CurrentThread`
    /// instance of the `Handle` does not exist anymore.
    pub fn spawn_std<F>(&self, future: F) -> Result<(), tokio_02::executor::SpawnError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
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
    pub fn status(&self) -> Result<(), executor_01::SpawnError> {
        self.0.status().map_err(compat::spawn_err)
    }
}

impl<T> tokio_02::executor::TypedExecutor<T> for Handle
where
    T: Future<Output = ()> + Send + 'static,
{
    fn spawn(&mut self, future: T) -> Result<(), tokio_02::executor::SpawnError> {
        Handle::spawn_std(self, future)
    }
}

impl<T> executor_01::TypedExecutor<T> for Handle
where
    T: Future01<Item = (), Error = ()> + Send + 'static,
{
    fn spawn(&mut self, future: T) -> Result<(), executor_01::SpawnError> {
        Handle::spawn(self, future)
    }
}

/// Error returned by the `run` function.
#[derive(Debug)]
pub struct RunError {
    inner: current_thread::RunError,
}

impl fmt::Display for RunError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{}", self.inner)
    }
}

impl Error for RunError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.inner.source()
    }
}

struct CompatExec {
    inner: current_thread::TaskExecutor,
}

impl Runtime {
    /// Returns a new runtime initialized with default configuration values.
    pub fn new() -> io::Result<Runtime> {
        Builder::new().build()
    }

    pub(super) fn new2(
        reactor_handle: driver::Handle,
        timer_handle: timer::Handle,
        clock: Clock,
        executor: CurrentThread<Parker>,
    ) -> io::Result<Runtime> {
        let compat = compat::Background::spawn(&clock)?;
        Ok(Runtime {
            reactor_handle,
            timer_handle,
            clock,
            executor,
            compat,
        })
    }

    /// Get a new handle to spawn futures on the single-threaded Tokio runtime
    ///
    /// Different to the runtime itself, the handle can be sent to different
    /// threads.
    pub fn handle(&self) -> Handle {
        Handle(self.executor.handle().clone())
    }

    /// Spawn a `futures` 0.1 future onto the single-threaded Tokio runtime.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::current_thread::Runtime;
    ///
    /// # fn dox() {
    /// // Create the runtime
    /// let mut rt = Runtime::new().unwrap();
    ///
    /// // Spawn a future onto the runtime
    /// rt.spawn(futures_01::future::lazy(|| {
    ///     println!("now running on a worker thread");
    ///     Ok(())
    /// }));
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn<F>(&mut self, future: F) -> &mut Self
    where
        F: Future01<Item = (), Error = ()> + 'static,
    {
        self.executor.spawn(future.compat().map(|_| ()));
        self
    }

    /// Spawn a `std::future` future onto the single-threaded Tokio runtime.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::current_thread::Runtime;
    ///
    /// # fn dox() {
    /// // Create the runtime
    /// let mut rt = Runtime::new().unwrap();
    ///
    /// // Spawn a future onto the runtime
    /// rt.spawn_std(async {
    ///     println!("now running on a worker thread");
    /// });
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn_std<F>(&mut self, future: F) -> &mut Self
    where
        F: Future<Output = ()> + 'static,
    {
        self.executor.spawn(future);
        self
    }

    /// Runs the provided `futures` 0.1 future, blocking the current thread
    /// until the future completes.
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
    where
        F: Future01,
    {
        self.enter(|executor| {
            // Run the provided future
            executor.block_on(f.compat())
        })
    }

    /// Runs the provided `std::future` future, blocking the current thread
    /// until the future completes.
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
    pub fn block_on_std<F>(&mut self, f: F) -> F::Output
    where
        F: Future,
    {
        self.enter(|executor| {
            // Run the provided future
            executor.block_on(f)
        })
    }

    /// Run the executor to completion, blocking the thread until **all**
    /// spawned futures have completed.
    pub fn run(&mut self) -> Result<(), RunError> {
        self.enter(|executor| executor.run())
            .map_err(|e| RunError { inner: e })
    }

    fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut current_thread::CurrentThread<Parker>) -> R,
    {
        let Runtime {
            ref reactor_handle,
            ref timer_handle,
            ref clock,
            ref mut executor,
            ref compat,
        } = *self;

        let mut enter = executor_01::enter().unwrap();
        // Set the default tokio 0.1 reactor to the background compat reactor.
        reactor_01::with_default(compat.reactor(), &mut enter, |enter| {
            // This will set the default handle and timer to use inside the closure
            // and run the future.
            let _reactor = driver::set_default(&reactor_handle);
            clock::with_default(clock, || {
                // Set up a default timer for tokio 0.1 compat.
                timer_02::with_default(compat.timer(), enter, |enter| {
                    let _timer = timer::set_default(&timer_handle);
                    // Set default executor for tokio 0.1 futures.
                    let mut compat_exec = CompatExec {
                        inner: current_thread::TaskExecutor::current(),
                    };
                    executor_01::with_default(&mut compat_exec, enter, |_enter| {
                        // The TaskExecutor is a fake executor that looks into the
                        // current single-threaded executor when used. This is a trick,
                        // because we need two mutable references to the executor (one
                        // to run the provided future, another to install as the default
                        // one). We use the fake one here as the default one.
                        let mut default_executor = current_thread::TaskExecutor::current();
                        tokio_02::executor::with_default(&mut default_executor, || f(executor))
                    })
                })
            })
        })
    }
}

impl executor_01::Executor for CompatExec {
    fn spawn(
        &mut self,
        future: Box<dyn futures_01::Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), executor_01::SpawnError> {
        let future = future.compat().map(|_| ());
        tokio_02::executor::Executor::spawn(&mut self.inner, Box::pin(future))
            .map_err(compat::spawn_err)
    }

    fn status(&self) -> Result<(), executor_01::SpawnError> {
        tokio_02::executor::Executor::status(&self.inner).map_err(compat::spawn_err)
    }
}
