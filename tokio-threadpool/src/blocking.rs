use worker::Worker;

use futures::Poll;

use std::error::Error;
use std::fmt;

/// Error raised by `blocking`.
pub struct BlockingError {
    _p: (),
}

/// Enter a blocking section of code.
///
/// The `blocking` function annotates a section of code that performs a blocking
/// operation, either by issuing a blocking syscall or by performing a long
/// running CPU-bound computation.
///
/// When the `blocking` function enters, it hands off the responsibility of
/// processing the current work queue to another thread. Then, it calls the
/// supplied closure. The closure is permitted to block indefinitely.
///
/// If the maximum number of concurrent `blocking` calls has been reached, then
/// `NotReady` is returned and the task is notified once existing `blocking`
/// calls complete. The maximum value is specified when creating a thread pool
/// using [`Builder::max_blocking`][build]
///
/// NB: The entire task that called `blocking` is blocked whenever the supplied
/// closure blocks, even if you have used future combinators such as `select` -
/// the other futures in this task will not make progress until the closure
/// returns.
/// If this is not desired, ensure that `blocking` runs in its own task (e.g.
/// using `futures::sync::oneshot::spawn`).
///
/// [build]: struct.Builder.html#method.max_blocking
///
/// # Return
///
/// When the blocking closure is executed, `Ok(Ready(T))` is returned, where
/// `T` is the closure's return value.
///
/// If the thread pool has shutdown, `Err` is returned.
///
/// If the number of concurrent `blocking` calls has reached the maximum,
/// `Ok(NotReady)` is returned and the current task is notified when a call to
/// `blocking` will succeed.
///
/// If `blocking` is called from outside the context of a Tokio thread pool,
/// `Err` is returned.
///
/// # Background
///
/// By default, the Tokio thread pool expects that tasks will only run for short
/// periods at a time before yielding back to the thread pool. This is the basic
/// premise of cooperative multitasking.
///
/// However, it is common to want to perform a blocking operation while
/// processing an asynchronous computation. Examples of blocking operation
/// include:
///
/// * Performing synchronous file operations (reading and writing).
/// * Blocking on acquiring a mutex.
/// * Performing a CPU bound computation, like cryptographic encryption or
///   decryption.
///
/// One option for dealing with blocking operations in an asynchronous context
/// is to use a thread pool dedicated to performing these operations. This not
/// ideal as it requires bidirectional message passing as well as a channel to
/// communicate which adds a level of buffering.
///
/// Instead, `blocking` hands off the responsibility of processing the work queue
/// to another thread. This hand off is light compared to a channel and does not
/// require buffering.
///
/// # Examples
///
/// Block on receiving a message from a `std` channel. This example is a little
/// silly as using the non-blocking channel from the `futures` crate would make
/// more sense. The blocking receive can be replaced with any blocking operation
/// that needs to be performed.
///
/// ```rust
/// # extern crate futures;
/// # extern crate tokio_threadpool;
///
/// use tokio_threadpool::{ThreadPool, blocking};
///
/// use futures::Future;
/// use futures::future::{lazy, poll_fn};
///
/// use std::sync::mpsc;
/// use std::thread;
/// use std::time::Duration;
///
/// pub fn main() {
///     // This is a *blocking* channel
///     let (tx, rx) = mpsc::channel();
///
///     // Spawn a thread to send a message
///     thread::spawn(move || {
///         thread::sleep(Duration::from_millis(500));
///         tx.send("hello").unwrap();
///     });
///
///     let pool = ThreadPool::new();
///
///     pool.spawn(lazy(move || {
///         // Because `blocking` returns `Poll`, it is intended to be used
///         // from the context of a `Future` implementation. Since we don't
///         // have a complicated requirement, we can use `poll_fn` in this
///         // case.
///         poll_fn(move || {
///             blocking(|| {
///                 let msg = rx.recv().unwrap();
///                 println!("message = {}", msg);
///             }).map_err(|_| panic!("the threadpool shut down"))
///         })
///     }));
///
///     // Wait for the task we just spawned to complete.
///     pool.shutdown_on_idle().wait().unwrap();
/// }
/// ```
pub fn blocking<F, T>(f: F) -> Poll<T, BlockingError>
where
    F: FnOnce() -> T,
{
    let res = Worker::with_current(|worker| {
        let worker = match worker {
            Some(worker) => worker,
            None => {
                return Err(BlockingError { _p: () });
            }
        };

        // Transition the worker state to blocking. This will exit the fn early
        // with `NotReady` if the pool does not have enough capacity to enter
        // blocking mode.
        worker.transition_to_blocking()
    });

    // If the transition cannot happen, exit early
    try_ready!(res);

    // Currently in blocking mode, so call the inner closure
    let ret = f();

    // Try to transition out of blocking mode. This is a fast path that takes
    // back ownership of the worker if the worker handoff didn't complete yet.
    Worker::with_current(|worker| {
        // Worker must be set since it was above.
        worker.unwrap().transition_from_blocking();
    });

    // Return the result
    Ok(ret.into())
}

impl fmt::Display for BlockingError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.description())
    }
}

impl fmt::Debug for BlockingError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BlockingError")
            .field("reason", &self.description())
            .finish()
    }
}

impl Error for BlockingError {
    fn description(&self) -> &str {
        "`blocking` annotation used from outside the context of a thread pool"
    }
}
