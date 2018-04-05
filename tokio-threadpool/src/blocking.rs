use worker::Worker;

use futures::Poll;

/// Error raised by `blocking`.
#[derive(Debug)]
pub struct BlockingError {
    _p: (),
}

/// Enter a blocking section of code.
///
/// The `blocking` function annotates a section of code that performs a blocking
/// operation, either by issuing a blocking syscall or by performing a long
/// running CPU bound computation.
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
/// [build]: struct.Builder.html#method.max_blocking
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
/// Instead, `blocking` hands off the responsiblity of processing the work queue
/// to another thread. This hand off is light compared to a channel and does not
/// require buffering.
///
/// # Panics
///
/// This function panics if not called from the context of a thread pool worker.
pub fn blocking<F, T>(f: F) -> Poll<T, BlockingError>
where F: FnOnce() -> T,
{
    let res = Worker::with_current(|worker| {
        let worker = worker.expect("not called from a runtime thread");

        // Transition the worker state to blocking. This will exit the fn early
        // with `NotRead` if the pool does not have enough capacity to enter
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
        worker.unwrap()
            .transition_from_blocking();
    });

    // Return the result
    Ok(ret.into())
}
