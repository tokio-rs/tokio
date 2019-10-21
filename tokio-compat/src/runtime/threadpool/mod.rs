mod background;
mod builder;
mod task_executor;
pub use builder::Builder;
pub use task_executor::TaskExecutor;

use super::compat;
use background::Background;

use futures_01::future::Future as Future01;
use futures_util::{compat::Future01CompatExt, future::FutureExt};
use std::{future::Future, io};
use tokio_executor::enter;
use tokio_executor::threadpool::{Sender, ThreadPool};
use tokio_executor_01 as executor_01;
use tokio_net::driver;
use tokio_timer::timer;

/// A thread pool runtime that can run tasks that use both `tokio` 0.1 and
/// `tokio` 0.2 APIs.
///
/// This functions similarly to the [`tokio::runtime::Runtime`][rt] struct in the
/// `tokio` crate. However, unlike that runtime, the `tokio-compat` runtime is
/// capable of running both `std::future::Future` tasks that use `tokio` 0.2
/// runtime services. and `futures` 0.1 tasks that use `tokio` 0.1 runtime
/// services.
///
/// [rt]: https://docs.rs/tokio/0.2.0-alpha.6/tokio/runtime/struct.Runtime.html
#[derive(Debug)]
pub struct Runtime {
    inner: Option<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// Task execution pool.
    pool: ThreadPool,

    /// Tracing dispatcher
    trace: tracing_core::Dispatch,

    /// Maintains a reactor and timer that are always running on a background
    /// thread. This is to support `runtime.block_on` w/o requiring the future
    /// to be `Send`.
    ///
    /// A dedicated background thread is required as the threadpool threads
    /// might not be running. However, this is a temporary work around.
    background: Background,

    /// Compatibility background thread.
    ///
    /// This maintains a `tokio` 0.1 timer and reactor to support running
    /// futures that use older tokio APIs.
    compat_bg: compat::Background,
}

#[derive(Clone, Debug)]
struct CompatSender<S>(S);

// ===== impl Runtime =====

/// Start the Tokio runtime using the supplied `futures` 0.1 future to bootstrap
/// execution.
///
/// This function is used to bootstrap the execution of a Tokio application. It
/// does the following:
///
/// * Start the Tokio runtime using a default configuration.
/// * Spawn the given future onto the thread pool.
/// * Block the current thread until the runtime shuts down.
///
/// Note that the function will not return immediately once `future` has
/// completed. Instead it waits for the entire runtime to become idle.
///
/// See the [module level][mod] documentation for more details.
///
/// # Examples
///
/// ```rust
/// use futures_01::{Future as Future01, Stream as Stream01};
/// use tokio_01::net::TcpListener;
///
/// # fn process<T>(_: T) -> Box<Future01<Item = (), Error = ()> + Send> {
/// # unimplemented!();
/// # }
/// # fn dox() {
/// # let addr = "127.0.0.1:8080".parse().unwrap();
/// let listener = TcpListener::bind(&addr).unwrap();
///
/// let server = listener.incoming()
///     .map_err(|e| println!("error = {:?}", e))
///     .for_each(|socket| {
///         tokio_01::spawn(process(socket))
///     });
///
/// tokio_compat::run(server);
/// # }
/// # pub fn main() {}
/// ```
///
/// # Panics
///
/// This function panics if called from the context of an executor.
///
/// [mod]: ../index.html
pub fn run<F>(future: F)
where
    F: Future01<Item = (), Error = ()> + Send + 'static,
{
    run_std(future.compat().map(|_| ()))
}

/// Start the Tokio runtime using the supplied `std::future` future to bootstrap
/// execution.
///
/// This function is used to bootstrap the execution of a Tokio application. It
/// does the following:
///
/// * Start the Tokio runtime using a default configuration.
/// * Spawn the given future onto the thread pool.
/// * Block the current thread until the runtime shuts down.
///
/// Note that the function will not return immediately once `future` has
/// completed. Instead it waits for the entire runtime to become idle.
///
/// See the [module level][mod] documentation for more details.
///
/// # Examples
///
/// ```rust
/// use futures_01::{Future as Future01, Stream as Stream01};
/// use tokio_01::net::TcpListener;
///
/// # fn process<T>(_: T) -> Box<Future01<Item = (), Error = ()> + Send> {
/// # unimplemented!();
/// # }
/// # fn dox() {
/// # let addr = "127.0.0.1:8080".parse().unwrap();
/// let listener = TcpListener::bind(&addr).unwrap();
///
/// let server = listener.incoming()
///     .map_err(|e| println!("error = {:?}", e))
///     .for_each(|socket| {
///         tokio_01::spawn(process(socket))
///     });
///
/// tokio_compat::run(server);
/// # }
/// # pub fn main() {}
/// ```
///
/// # Panics
///
/// This function panics if called from the context of an executor.
///
/// [mod]: ../index.html
pub fn run_std<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    // Check enter before creating a new Runtime...
    let runtime = Runtime::new().expect("failed to start new Runtime");
    runtime.spawn_std(future);
    runtime.shutdown_on_idle();
}

impl Runtime {
    /// Create a new runtime instance with default configuration values.
    ///
    /// This results in a reactor, thread pool, and timer being initialized. The
    /// thread pool will not spawn any worker threads until it needs to, i.e.
    /// tasks are scheduled to run.
    ///
    /// Most users will not need to call this function directly, instead they
    /// will use [`tokio_compat::run`](fn.run.html).
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// # Examples
    ///
    /// Creating a new `Runtime` with default configuration values.
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// // Use the runtime...
    /// ```
    ///
    /// [mod]: index.html
    pub fn new() -> io::Result<Self> {
        Builder::new().build()
    }

    /// Return a handle to the runtime's executor.
    ///
    /// The returned handle can be used to spawn both `futures` 0.1 and
    /// `std::future` tasks that run on this runtime.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// let executor_handle = rt.executor();
    ///
    /// // use `executor_handle`
    /// ```
    pub fn executor(&self) -> TaskExecutor {
        let inner = CompatSender(self.inner().pool.sender().clone());
        TaskExecutor { inner }
    }

    /// Spawn a `futures` 0.1 future onto the Tokio runtime.
    ///
    /// This spawns the given future onto the runtime's executor, usually a
    /// thread pool. The thread pool is then responsible for polling the future
    /// until it completes.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    ///
    /// fn main() {
    ///    // Create the runtime
    ///    let rt = Runtime::new().unwrap();
    ///
    ///    // Spawn a future onto the runtime
    ///    rt.spawn(futures_01::future::lazy(|| {
    ///        println!("now running on a worker thread");
    ///        Ok(())
    ///    }));
    ///
    ///    rt.shutdown_on_idle();
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn<F>(&self, future: F) -> &Self
    where
        F: Future01<Item = (), Error = ()> + Send + 'static,
    {
        self.inner().pool.spawn(future.compat().map(|_| ()));
        self
    }

    /// Spawn a `std::future` future onto the Tokio runtime.
    ///
    /// This spawns the given future onto the runtime's executor, usually a
    /// thread pool. The thread pool is then responsible for polling the future
    /// until it completes.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    ///
    /// fn main() {
    ///    // Create the runtime
    ///    let rt = Runtime::new().unwrap();
    ///
    ///    // Spawn a future onto the runtime
    ///    rt.spawn_std(async {
    ///        println!("now running on a worker thread");
    ///    });
    ///
    ///    rt.shutdown_on_idle();
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn_std<F>(&self, future: F) -> &Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.inner().pool.spawn(future);
        self
    }

    /// Run a `futures` 0.1 future to completion on the Tokio runtime.
    ///
    /// This runs the given future on the runtime, blocking until it is
    /// complete, and yielding its resolved result. Any tasks or timers which
    /// the future spawns internally will be executed on the runtime.
    ///
    /// This method should not be called from an asynchronous context.
    ///
    /// # Panics
    ///
    /// This function panics if the executor is at capacity, if the provided
    /// future panics, or if called within an asynchronous execution context.
    pub fn block_on<F>(&self, future: F) -> Result<F::Item, F::Error>
    where
        F: Future01,
    {
        self.block_on_std(future.compat())
    }

    /// Run a `std::future` future to completion on the Tokio runtime.
    ///
    /// This runs the given future on the runtime, blocking until it is
    /// complete, and yielding its resolved result. Any tasks or timers which
    /// the future spawns internally will be executed on the runtime.
    ///
    /// This method should not be called from an asynchronous context.
    ///
    /// # Panics
    ///
    /// This function panics if the executor is at capacity, if the provided
    /// future panics, or if called within an asynchronous execution context.
    pub fn block_on_std<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        let mut entered = enter().expect("nested block_on");
        let mut old_entered = executor_01::enter().expect("nested block_on");
        let bg = &self.inner().background;
        let trace = &self.inner().trace;

        tokio_executor::with_default(&mut self.inner().pool.sender(), || {
            executor_01::with_default(
                &mut CompatSender(self.inner().pool.sender()),
                &mut old_entered,
                |_old_entered| {
                    let _reactor = driver::set_default(bg.reactor());
                    let _timer = timer::set_default(bg.timer());
                    tracing_core::dispatcher::with_default(trace, || entered.block_on(future))
                },
            )
        })
    }

    /// Signals the runtime to shutdown once it becomes idle.
    ///
    /// Blocks the current thread until the shutdown operation has completed.
    /// This function can be used to perform a graceful shutdown of the runtime.
    ///
    /// The runtime enters an idle state once **all** of the following occur.
    ///
    /// * The thread pool has no tasks to execute, i.e., all tasks that were
    ///   spawned have completed.
    /// * The reactor is not managing any I/O resources.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// // Use the runtime...
    ///
    /// // Shutdown the runtime
    /// rt.shutdown_on_idle();
    /// ```
    ///
    /// [mod]: index.html
    pub fn shutdown_on_idle(mut self) {
        let mut e = tokio_executor::enter().unwrap();

        let inner = self.inner.take().unwrap();
        e.block_on(inner.pool.shutdown_on_idle());
    }

    fn inner(&self) -> &Inner {
        self.inner.as_ref().unwrap()
    }
}

fn translate_spawn_err(new: tokio_executor::SpawnError) -> executor_01::SpawnError {
    match new {
        _ if new.is_shutdown() => executor_01::SpawnError::shutdown(),
        _ if new.is_at_capacity() => executor_01::SpawnError::at_capacity(),
        e => unreachable!("weird spawn error {:?}", e),
    }
}

impl executor_01::Executor for CompatSender<&'_ Sender> {
    fn spawn(
        &mut self,
        future: Box<dyn futures_01::Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), executor_01::SpawnError> {
        self.0
            .spawn(future.compat().map(|_| ()))
            .map_err(translate_spawn_err)
    }

    fn status(&self) -> Result<(), executor_01::SpawnError> {
        tokio_executor::Executor::status(&self.0).map_err(translate_spawn_err)
    }
}

impl executor_01::Executor for CompatSender<Sender> {
    fn spawn(
        &mut self,
        future: Box<dyn futures_01::Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), executor_01::SpawnError> {
        self.0
            .spawn(future.compat().map(|_| ()))
            .map_err(translate_spawn_err)
    }

    fn status(&self) -> Result<(), executor_01::SpawnError> {
        tokio_executor::Executor::status(&self.0).map_err(translate_spawn_err)
    }
}

impl<T> executor_01::TypedExecutor<T> for CompatSender<Sender>
where
    T: Future01<Item = (), Error = ()> + Send + 'static,
{
    fn spawn(&mut self, future: T) -> Result<(), executor_01::SpawnError> {
        let future = Box::pin(future.compat().map(|_| ()));
        // Use the `tokio` 0.2 `TypedExecutor` impl so we don't have to box the
        // future twice (once to spawn it using `Executor01::spawn` and a second
        // time to pin the compat future).
        self.0.spawn(future).map_err(compat::spawn_err)
    }

    fn status(&self) -> Result<(), executor_01::SpawnError> {
        executor_01::Executor::status(self)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            drop(inner);
        }
    }
}

#[cfg(test)]
mod tests;
