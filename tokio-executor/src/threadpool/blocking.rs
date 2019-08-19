use super::worker::Worker;

use std::error::Error;
use std::fmt;
use std::task::Poll;

/// Error raised by `blocking`.
pub struct BlockingError(pub(crate) Kind);

#[derive(PartialEq)]
pub(crate) enum Kind {
    NotWorker,
    NoCapacity,
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
/// use tokio_executor::threadpool::{ThreadPool, blocking};
///
/// use futures_util::future::poll_fn;
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
///     pool.spawn(async move {
///         // Because `blocking` returns `Poll`, it is intended to be used
///         // from the context of a `Future` implementation. Since we don't
///         // have a complicated requirement, we can use `poll_fn` in this
///         // case.
///         poll_fn(move |_| {
///             blocking(|| {
///                 let msg = rx.recv().unwrap();
///                 println!("message = {}", msg);
///             }).map_err(|_| panic!("the threadpool shut down"))
///         }).await;
///     });
///
///     // Wait for the task we just spawned to complete.
///     pool.shutdown_on_idle().wait();
/// }
/// ```
pub fn blocking<F, T>(f: F) -> Poll<Result<T, BlockingError>>
where
    F: FnOnce() -> T,
{
    match enter_blocking_section() {
        Ok(()) => {}
        Err(BlockingError(Kind::NoCapacity)) => return Poll::Pending,
        Err(e @ BlockingError(Kind::NotWorker)) => return Poll::Ready(Err(e)),
    }

    // Currently in blocking mode, so call the inner closure
    //
    // "Exit" the current executor in case the blocking function wants
    // to call a different executor.
    let ret = crate::exit(move || f());

    if let Err(e) = exit_blocking_section() {
        return Poll::Ready(Err(e));
    }

    // Return the result
    Poll::Ready(Ok(ret))
}

/// Enter a blocking section of code.  Returns `BlockingError` if called from a
/// thread not associated with a Tokio thread pool, or if there is no capacity.
pub fn enter_blocking_section() -> Result<(), BlockingError> {
    Worker::with_current(|worker| {
        if let Some(worker) = worker {
            worker.transition_to_blocking()
        } else {
            Err(BlockingError(Kind::NotWorker))
        }
    })
}

/// Try to transition out of blocking mode. This is a fast path that takes back
/// ownership of the worker if the worker handoff didn't complete yet.  Returns
/// `BlockingError` if called from a thread not associated with a Tokio thread
/// pool.
pub fn exit_blocking_section() -> Result<(), BlockingError> {
    Worker::with_current(|worker| {
        if let Some(worker) = worker {
            worker.transition_from_blocking();
            Ok(())
        } else {
            Err(BlockingError(Kind::NotWorker))
        }
    })
}

impl BlockingError {
    /// Return true if the error is due to a thread calling `blocking` or
    /// `enter_blocking_section` that is not a tokio threadpool worker thread.
    pub fn is_not_worker(&self) -> bool {
        self.0 == Kind::NotWorker
    }
}

impl fmt::Display for BlockingError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Kind::NotWorker => write!(
                fmt,
                "blocking used from outside the context of a thread pool"
            ),
            Kind::NoCapacity => write!(fmt, "max blocking threads threshold was reached"),
        }
    }
}

impl fmt::Debug for BlockingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockingError")
            .field("reason", &format!("{}", self))
            .finish()
    }
}

impl Error for BlockingError {}
