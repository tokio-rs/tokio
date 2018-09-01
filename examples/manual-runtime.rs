//! An example how to manually assemble a runtime and run some tasks on it.
//!
//! This is closer to the single-threaded runtime than the default tokio one, as it is simpler to
//! grasp. There are conceptually similar, but the multi-threaded one would be more code. If you
//! just want to *use* a single-threaded runtime, use the one provided by tokio directly
//! (`tokio::runtime::current_thread::Runtime::new()`. This is a demonstration only.
//!
//! Note that the error handling is a bit left out. Also, the `run` could be modified to return the
//! result of the provided future.

extern crate futures;
extern crate tokio;
extern crate tokio_current_thread;
extern crate tokio_executor;
extern crate tokio_reactor;
extern crate tokio_timer;

use std::io::Error as IoError;
use std::time::{Duration, Instant};

use futures::{future, Future};
use tokio_current_thread::CurrentThread;
use tokio_reactor::Reactor;
use tokio_timer::timer::{self, Timer};

/// Creates a "runtime".
///
/// This is similar to running `tokio::runtime::current_thread::Runtime::new()`.
fn run<F: Future<Item = (), Error = ()>>(f: F) -> Result<(), IoError> {
    // We need a reactor to receive events about IO objects from kernel
    let reactor = Reactor::new()?;
    let reactor_handle = reactor.handle();
    // Place a timer wheel on top of the reactor. If there are no timeouts to fire, it'll let the
    // reactor pick up some new external events.
    let timer = Timer::new(reactor);
    let timer_handle = timer.handle();
    // And now put a single-threaded executor on top of the timer. When there are no futures ready
    // to do something, it'll let the timer or the reactor generate some new stimuli for the
    // futures to continue in their life.
    let mut executor = CurrentThread::new_with_park(timer);
    // Binds an executor to this thread
    let mut enter = tokio_executor::enter().expect("Multiple executors at once");
    // This will set the default handle and timer to use inside the closure and run the future.
    tokio_reactor::with_default(&reactor_handle, &mut enter, |enter| {
        timer::with_default(&timer_handle, enter, |enter| {
            // The TaskExecutor is a fake executor that looks into the current single-threaded
            // executor when used. This is a trick, because we need two mutable references to the
            // executor (one to run the provided future, another to install as the default one). We
            // use the fake one here as the default one.
            let mut default_executor = tokio_current_thread::TaskExecutor::current();
            tokio_executor::with_default(&mut default_executor, enter, |enter| {
                let mut executor = executor.enter(enter);
                // Run the provided future
                executor.block_on(f).unwrap();
                // Run all the other futures that are still left in the executor
                executor.run().unwrap();
            });
        });
    });
    Ok(())
}

fn main() {
    run(future::lazy(|| {
        // Here comes the application logic. It can spawn further tasks by tokio_current_thread::spawn().
        // It also can use the default reactor and create timeouts.

        // Connect somewhere. And then do nothing with it. Yes, useless.
        //
        // This will use the default reactor which runs in the current thread.
        let connect = tokio::net::TcpStream::connect(&"127.0.0.1:53".parse().unwrap())
            .map(|_| println!("Connected"))
            .map_err(|e| println!("Failed to connect: {}", e));
        // We can spawn it without requiring Send. This would panic if we run it outside of the
        // `run` (or outside of anything else)
        tokio_current_thread::spawn(connect);

        // We can also create timeouts.
        let deadline = tokio::timer::Delay::new(Instant::now() + Duration::from_secs(5))
            .map(|()| println!("5 seconds are over"))
            .map_err(|e| println!("Failed to wait: {}", e));
        // We can spawn on the default executor, which is also the local one.
        tokio::executor::spawn(deadline);
        Ok(())
    })).unwrap();
}
