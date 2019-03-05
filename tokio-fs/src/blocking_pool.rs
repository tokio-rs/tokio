use futures::future;
use futures::prelude::*;
use futures::sync::mpsc;
use futures::sync::oneshot;

use std::any::Any;
use std::panic;
use std::thread;

lazy_static! {
    /// A thread pool for executing blocking functions.
    static ref BLOCKING_POOL: tokio_threadpool::ThreadPool = {
        tokio_threadpool::Builder::new().pool_size(1).build()
    };

    /// Schedules blocking functions for execution on the thread pool.
    static ref SENDER: mpsc::UnboundedSender<Box<FnMut() + Send + 'static>> = {
        let (tx, rx) = mpsc::unbounded();

        BLOCKING_POOL.spawn(rx.for_each(|mut f: Box<FnMut() + Send + 'static>| {
            BLOCKING_POOL.spawn(future::poll_fn(move || {
                tokio_threadpool::blocking(|| f()).map_err(|_| ())
            }));
            Ok(())
        }));

        tx
    };
}

/// Spawns a blocking function onto the blocking pool.
pub fn spawn_blocking<T, F>(f: F) -> BlockingFuture<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    // Create a channel that will send back the result of `f`.
    let (tx, rx) = oneshot::channel();

    // Wrap the blocking function into one that returns the result back through the channel.
    //
    // Unfortunately, since `tokio_threadpool::blocking(|| f())` may have to be retried a few
    // times before it completes, that means `f` cannot really be a `FnOnce`. In order to get
    // around the problem, we turn `f` into a `FnMut` that panics if called more than once.
    let mut tx = Some(tx);
    let mut f = Some(f);
    let blocking_f = move || {
        let f = f.take().unwrap();
        let tx = tx.take().unwrap();

        // If the receiver side has been dropped, don't execute the function at all.
        if !tx.is_canceled() {
            // Catch any panics inside the blocking function so that they can be returned in the
            // error case by the `BlockingFuture`.
            let res = panic::catch_unwind(panic::AssertUnwindSafe(f));
            let _ = tx.send(res);
        }
    };

    // Send the blocking function to the thread pool for execution.
    SENDER.unbounded_send(Box::new(blocking_f)).unwrap();

    BlockingFuture { rx }
}

/// Future that resolves to the result of a blocking function.
#[derive(Debug)]
pub struct BlockingFuture<T> {
    rx: oneshot::Receiver<thread::Result<T>>,
}

impl<T> Future for BlockingFuture<T> {
    type Item = T;
    type Error = Box<Any + Send + 'static>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.rx.poll().unwrap() {
            Async::NotReady => Ok(Async::NotReady),
            Async::Ready(Ok(t)) => Ok(Async::Ready(t)),
            Async::Ready(Err(err)) => Err(err),
        }
    }
}
