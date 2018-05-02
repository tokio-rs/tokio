use executor::current_thread::{self, CurrentThread};

use tokio_reactor::{self, Reactor};
use tokio_timer::timer::{self, Timer};
use tokio_executor;

use futures::Future;

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
    executor: CurrentThread<Timer<Reactor>>,
}

/// Error returned by the `run` function.
#[derive(Debug)]
pub struct RunError {
    inner: current_thread::RunError,
}

impl Runtime {
    /// Returns a new runtime initialized with default configuration values.
    pub fn new() -> io::Result<Runtime> {
        // We need a reactor to receive events about IO objects from kernel
        let reactor = Reactor::new()?;
        let reactor_handle = reactor.handle();

        // Place a timer wheel on top of the reactor. If there are no timeouts to fire, it'll let the
        // reactor pick up some new external events.
        let timer = Timer::new(reactor);
        let timer_handle = timer.handle();

        // And now put a single-threaded executor on top of the timer. When there are no futures ready
        // to do something, it'll let the timer or the reactor to generate some new stimuli for the
        // futures to continue in their life.
        let executor = CurrentThread::new_with_park(timer);

        let runtime = Runtime { reactor_handle, timer_handle, executor };
        Ok(runtime)
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
        let Runtime { ref reactor_handle, ref timer_handle, ref mut executor } = *self;

        // Binds an executor to this thread
        let mut enter = tokio_executor::enter().expect("Multiple executors at once");

        // This will set the default handle and timer to use inside the closure
        // and run the future.
        tokio_reactor::with_default(&reactor_handle, &mut enter, |enter| {
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
    }
}
