use executor::current_thread::{self, CurrentThread};

use tokio_reactor::{self, Reactor};
use tokio_timer::timer::{self, Timer};
use tokio_executor;

use futures::Future;

use std::io;

/// Single-threaded runtime provides a way to start reactor
/// and executor on the same thread.
///
/// The single-threaded runtime is not `Send` and cannot be
/// safely moved to other threads.
///
/// # Examples
///
/// Creating a new `Runtime` with default configuration values.
///
/// ```
/// use tokio::runtime::current_thread::Runtime;
/// use tokio::prelude::*;
///
/// let mut runtime = Runtime::new().unwrap();
///
/// // Use the runtime...
/// // runtime.block_on(f); // where f is a future
/// ```
#[derive(Debug)]
pub struct Runtime {
    reactor_handle: tokio_reactor::Handle,
    timer_handle: timer::Handle,
    executor: CurrentThread<Timer<Reactor>>,
}

impl Runtime {
    /// Returns a new single-threaded runtime initialized with default
    /// configuration values.
    pub fn new() -> io::Result<Runtime> {
        // We need a reactor to receive events about IO objects from kernel
        let reactor = Reactor::new()?;
        let reactor_handle = reactor.handle();

        // Place a timer wheel on top of the reactor. If there are no timeouts to fire, it'll let the
        // reactor pick up new some new external events.
        let timer = Timer::new(reactor);
        let timer_handle = timer.handle();

        // And now put a single-threaded executor on top of the timer. When there are no futures ready
        // to do something, it'll let the timer or the reactor to generate some new stimuli for the
        // futures to continue in their life.
        let executor = CurrentThread::new_with_park(timer);

        let runtime = Runtime { reactor_handle, timer_handle, executor };
        Ok(runtime)
    }

    /// Blocks and runs the given future.
    ///
    /// This is similar to running a runtime, but uses only the current thread.
    pub fn block_on<F: Future<Item = (), Error = ()>>(&mut self, f: F) -> () {
        let Runtime { ref reactor_handle, ref timer_handle, ref mut executor } = *self;

        // Binds an executor to this thread
        let mut enter = tokio_executor::enter().expect("Multiple executors at once");

        // This will set the default handle and timer to use inside the closure and run the future.
        tokio_reactor::with_default(&reactor_handle, &mut enter, |enter| {
            timer::with_default(&timer_handle, enter, |enter| {
                // The TaskExecutor is a fake executor that looks into the current single-threaded
                // executor when used. This is a trick, because we need two mutable references to the
                // executor (one to run the provided future, another to install as the default one). We
                // use the fake one here as the default one.
                let mut default_executor = current_thread::TaskExecutor::current();
                tokio_executor::with_default(&mut default_executor, enter, |enter| {
                    let mut executor = executor.enter(enter);
                    // Run the provided future
                    executor.block_on(f).unwrap();
                });
            });
        });
    }

    /// Blocks and runs all the other futures that are still left in the executor
    ///
    /// This is similar to running a runtime, but uses only the current thread.
    fn shutdown_on_idle(&mut self) -> () {
        let Runtime { ref reactor_handle, ref timer_handle, ref mut executor } = *self;

        // Binds an executor to this thread
        let mut enter = tokio_executor::enter().expect("Multiple executors at once");

        // This will set the default handle and timer to use inside the closure and run the future.
        tokio_reactor::with_default(&reactor_handle, &mut enter, |enter| {
            timer::with_default(&timer_handle, enter, |enter| {
                // The TaskExecutor is a fake executor that looks into the current single-threaded
                // executor when used. This is a trick, because we need two mutable references to the
                // executor (one to run the provided future, another to install as the default one). We
                // use the fake one here as the default one.
                let mut default_executor = current_thread::TaskExecutor::current();
                tokio_executor::with_default(&mut default_executor, enter, |enter| {
                    let mut executor = executor.enter(enter);
                    // Run all the other futures that are still left in the executor
                    executor.run().unwrap();
                });
            });
        });
    }

    /// Blocks and runs the given future untill there are no other futures left in the executor
    ///
    /// This is similar to running a runtime, but uses only the current thread.
    pub fn run<F: Future<Item = (), Error = ()>>(&mut self, f: F) -> () {
        self.block_on(f);
        self.shutdown_on_idle();
    }
}
