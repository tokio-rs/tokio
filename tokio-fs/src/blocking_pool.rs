//! A simple thread pool for executing blocking functions.

use futures::future;
use futures::prelude::*;
use futures::sync::oneshot;
use tokio_threadpool;

// TODO(stjepang): The current implementation is based on tokio-threadpool and is just a simple
// hack that works, but doesn't deliver great performance. We'll need a custom thread pool
// implementation here.

lazy_static::lazy_static! {
    /// A thread pool for executing blocking tasks.
    static ref POOL: tokio_threadpool::ThreadPool = {
        tokio_threadpool::Builder::new().pool_size(1).build()
    };
}

/// Creates a blocking task.
///
/// The task will start executing the first time the returned future is polled. The first
/// invocation of `poll()` will always return `Ok(Async::NotReady)`.
///
/// Dropping the returned future will cancel the task.
pub fn blocking<T, E, F>(mut f: F) -> Blocking<T, E>
where
    F: Future<Item = T, Error = E> + Send + 'static,
    T: Send + 'static,
    E: Send + 'static,
{
    // Create a channel that will send back the result of `f`.
    let (result_tx, result_rx) = oneshot::channel();

    // Create a channel that will start the future.
    let (start_tx, start_rx) = oneshot::channel();

    // Always poll the future inside an invocation of `blocking()`.
    let f = future::poll_fn(
        move || match tokio_threadpool::blocking(|| f.poll()).unwrap() {
            Async::Ready(v) => v,
            Async::NotReady => Ok(Async::NotReady),
        },
    );

    POOL.spawn(
        // First wait for the start signal.
        start_rx
            // Only continue if the start signal was received.
            // Note that if `Blocking` gets dropped, the task is cancelled.
            .and_then(|()| {
                // Poll `f` and then send the result through the channel.
                f.then(|res| {
                    let _ = result_tx.send(res);
                    Ok(())
                })
            })
            .map_err(|_| ()),
    );

    Blocking {
        result_rx,
        start_tx: Some(start_tx),
    }
}

/// Future that resolves to the result of a blocking task.
///
/// The task will start executing the first time this future is polled. The first invocation of
/// `poll()` will always return `Ok(Async::NotReady)`.
///
/// Dropping this future will cancel the task.
#[derive(Debug)]
pub struct Blocking<T, E> {
    result_rx: oneshot::Receiver<Result<T, E>>,
    start_tx: Option<oneshot::Sender<()>>,
}

impl<T, E> Future for Blocking<T, E> {
    type Item = T;
    type Error = E;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // If this is the first invocation of `poll()`, start the task.
        if let Some(start_tx) = self.start_tx.take() {
            // Poll the result so that the current task gets woken when the task completes.
            assert!(self.result_rx.poll().unwrap().is_not_ready());
            // Send the start signal.
            start_tx.send(()).unwrap();
            Ok(Async::NotReady)
        } else {
            match self.result_rx.poll().expect("the blocking task was lost") {
                Async::Ready(Ok(t)) => Ok(Async::Ready(t)),
                Async::Ready(Err(e)) => Err(e),
                Async::NotReady => Ok(Async::NotReady),
            }
        }
    }
}
