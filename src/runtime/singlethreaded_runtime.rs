use executor::current_thread::{self, CurrentThread};

use tokio_reactor::{self, Reactor};
use tokio_timer::timer::{self, Timer};
use tokio_executor::{self, Enter};

use futures::Future;

use std::cell::RefCell;
use std::io;

///
/// # Examples
///
/// Creating a new `Runtime` with default configuration values.
///
/// ```
/// use tokio::runtime::SinglethreadedRuntime;
/// use tokio::prelude::*;
///
/// let rt = SinglethreadedRuntime::new().unwrap();
///
/// // Use the runtime...
/// // rt.block_on(f); // where f is a future
/// ```
#[derive(Debug)]
pub struct SinglethreadedRuntime {
    reactor_handle: tokio_reactor::Handle,
    timer_handle: timer::Handle,
    executor: RefCell< CurrentThread<Timer<Reactor>> >,
    enter: RefCell< Enter >,
}

impl SinglethreadedRuntime {
    /// Returns a new singlethreaded runtime initialized with default
    /// configuration values.
    pub fn new() -> io::Result<SinglethreadedRuntime> {
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
        let executor = RefCell::new(executor);

        // Binds an executor to this thread
        let enter = tokio_executor::enter().expect("Multiple executors at once");
        let enter = RefCell::new(enter);

        let runtime = SinglethreadedRuntime { reactor_handle, timer_handle, executor, enter };
        Ok(runtime)
    }

    /// Blocks and runs the given future.
    ///
    /// This is similar to running a runtime, but uses only the current thread.
    pub fn block_on<F: Future<Item = (), Error = ()>>(&self, f: F) -> () {
        let reactor_handle = self.reactor_handle.clone();
        let timer_handle = self.timer_handle.clone();
        let mut executor = self.executor.borrow_mut();
        let mut enter = self.enter.borrow_mut();

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
                    // Run all the other futures that are still left in the executor
                    executor.run().unwrap();
                });
            });
        });
    }
}
