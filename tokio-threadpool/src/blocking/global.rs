use super::{Blocking, BlockingError, DEFAULT_BLOCKING};
use futures::Poll;
use std::cell::Cell;
use tokio_executor::Enter;

thread_local! {
    static CURRENT: Cell<*const dyn Blocking> = Cell::new(&DEFAULT_BLOCKING as *const _);
}

/// Set the default executor for the duration of the closure
///
/// # Panics
///
/// This function panics if there already is a default executor set.
pub fn with_default<B, F, R>(blocking: &B, enter: &mut Enter, f: F) -> R
where
    B: Blocking,
    F: FnOnce(&mut Enter) -> R,
{
    CURRENT.with(|cell| {
        struct Reset<'a> {
            cell: &'a Cell<*const dyn Blocking>,
            prior: *const dyn Blocking,
        };

        impl<'a> Drop for Reset<'a> {
            fn drop(&mut self) {
                self.cell.set(self.prior);
            }
        }

        unsafe fn hide_lt<'a>(p: *const (dyn Blocking + 'a)) -> *const (dyn Blocking + 'static) {
            use std::mem;
            mem::transmute(p)
        }

        // While scary, this is safe. The function takes a
        // `&mut Executor`, which guarantees that the reference lives for the
        // duration of `with_default`.
        //
        // Because we are always clearing the TLS value at the end of the
        // function, we can cast the reference to 'static which thread-local
        // cells require.
        let blocking = unsafe { hide_lt(blocking as &_ as *const _) };

        let prior = cell.replace(blocking);
        let _reset = Reset { cell, prior };

        f(enter)
    })
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
    CURRENT.with(|cell| {
        let blocking = unsafe { &*(cell.get()) };

        // Object-safety workaround: the `Blocking` trait must be object-safe,
        // since we use a trait object in the thread-local. However, a blocking
        // _operation_ will be generic over the return type of the blocking
        // function. Therefore, rather than passing a function with a return
        // type to `Blocking::run_blocking`, we pass a _new_ closure which
        // doesn't have a return value. That closure invokes the blocking
        // function and assigns its value to `ret`, which we then unpack when
        // the blocking call finishes.
        let mut f = Some(f);
        let mut ret = None;
        let ret2 = &mut ret;
        let mut run = move || {
            let f = f
                .take()
                .expect("blocking closure invoked twice; this is a bug!");
            *ret2 = Some((f)());
        };

        try_ready!(blocking.run_blocking(&mut run));

        // Return the result
        let ret =
            ret.expect("blocking function finished, but return value was unset; this is a bug!");
        Ok(ret.into())
    })
}
