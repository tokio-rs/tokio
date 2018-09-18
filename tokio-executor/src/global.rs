use super::{Executor, Enter, SpawnError};

use futures::{Async, Future, Poll};
use futures::sync::oneshot;

use std::cell::Cell;

/// Executes futures on the default executor for the current execution context.
///
/// `DefaultExecutor` implements `Executor` and can be used to spawn futures
/// without referencing a specific executor.
///
/// When an executor starts, it sets the `DefaultExecutor` handle to point to an
/// executor (usually itself) that is used to spawn new tasks.
///
/// The current `DefaultExecutor` reference is tracked using a thread-local
/// variable and is set using `tokio_executor::with_default`
#[derive(Debug, Clone)]
pub struct DefaultExecutor {
    _dummy: (),
}


/// Future returned by the [`spawn_handle`] function.
///
/// A `SpawnHandle` will finish with either the `Item` type of the spawned
/// future or a [`SpawnHandleError`] which either contains the spawned future's
/// `Error` type or indicates that the spawned future was canceled before it
/// completed.
///
/// In addition, the [`cancel`] method will cancel the spawned future, causing
/// it to finish executing the next time it is polled.
///
/// [`spawn_handle`]: fn.spawn_handle.html
/// [`SpawnHandleError`]: struct.SpawnHandleError.html
/// [`cancel`]: #method.cancel
#[derive(Debug)]
pub struct SpawnHandle<T, E> {
    cancel_tx: oneshot::Sender<()>,
    rx: oneshot::Receiver<Result<T, E>>,
}

/// Errors returned by `SpawnHandle`.
#[derive(Debug)]
pub struct SpawnHandleError<E> {
    kind: SpawnHandleErrorKind<E>,
}

#[derive(Debug)]
enum SpawnHandleErrorKind<E> {
    Inner(E),
    Canceled,
}

#[derive(Debug)]
struct SpawnedWithHandle<T: Future> {
    cancel_rx: oneshot::Receiver<()>,
    tx: Option<oneshot::Sender<Result<T::Item, T::Error>>>,
    future: T,
}

impl DefaultExecutor {
    /// Returns a handle to the default executor for the current context.
    ///
    /// Futures may be spawned onto the default executor using this handle.
    ///
    /// The returned handle will reference whichever executor is configured as
    /// the default **at the time `spawn` is called**. This enables
    /// `DefaultExecutor::current()` to be called before an execution context is
    /// setup, then passed **into** an execution context before it is used.
    ///
    /// This is also true for sending the handle across threads, so calling
    /// `DefaultExecutor::current()` on thread A and then sending the result to
    /// thread B will _not_ reference the default executor that was set on thread A.
    pub fn current() -> DefaultExecutor {
        DefaultExecutor {
            _dummy: (),
        }
    }

    #[inline]
    fn with_current<F: FnOnce(&mut Executor) -> R, R>(f: F) -> Option<R> {
        EXECUTOR.with(|current_executor| {
            match current_executor.replace(State::Active) {
                State::Ready(executor_ptr) => {
                    let executor = unsafe { &mut *executor_ptr };
                    let result = f(executor);
                    current_executor.set(State::Ready(executor_ptr));
                    Some(result)
                },
                State::Empty | State::Active => None,
            }
        })
    }
}

#[derive(Clone, Copy)]
enum State {
    // default executor not defined
    Empty,
    // default executor is defined and ready to be used
    Ready(*mut Executor),
    // default executor is currently active (used to detect recursive calls)
    Active
}

/// Thread-local tracking the current executor
thread_local!(static EXECUTOR: Cell<State> = Cell::new(State::Empty));

// ===== impl DefaultExecutor =====

impl super::Executor for DefaultExecutor {
    fn spawn(&mut self, future: Box<Future<Item = (), Error = ()> + Send>)
        -> Result<(), SpawnError>
    {
        DefaultExecutor::with_current(|executor| executor.spawn(future))
            .unwrap_or_else(|| Err(SpawnError::shutdown()))
    }

    fn status(&self) -> Result<(), SpawnError> {
        DefaultExecutor::with_current(|executor| executor.status())
            .unwrap_or_else(|| Err(SpawnError::shutdown()))
    }
}

// ===== global spawn fns =====

/// Submits a future for execution on the default executor -- usually a
/// threadpool.
///
/// Futures are lazy constructs. When they are defined, no work happens. In
/// order for the logic defined by the future to be run, the future must be
/// spawned on an executor. This function is the easiest way to do so.
///
/// This function must be called from an execution context, i.e. from a future
/// that has been already spawned onto an executor.
///
/// Once spawned, the future will execute. The details of how that happens is
/// left up to the executor instance. If the executor is a thread pool, the
/// future will be pushed onto a queue that a worker thread polls from. If the
/// executor is a "current thread" executor, the future might be polled
/// immediately from within the call to `spawn` or it might be pushed onto an
/// internal queue.
///
/// # Panics
///
/// This function will panic if the default executor is not set or if spawning
/// onto the default executor returns an error. To avoid the panic, use the
/// `DefaultExecutor` handle directly.
///
/// # Examples
///
/// ```rust
/// # extern crate futures;
/// # extern crate tokio_executor;
/// # use tokio_executor::spawn;
/// # pub fn dox() {
/// use futures::future::lazy;
///
/// spawn(lazy(|| {
///     println!("running on the default executor");
///     Ok(())
/// }));
/// # }
/// # pub fn main() {}
/// ```
pub fn spawn<T>(future: T)
    where T: Future<Item = (), Error = ()> + Send + 'static,
{
    DefaultExecutor::current().spawn(Box::new(future))
        .unwrap()
}

/// Submits a future for execution on the default executor, returning a
/// [`SpawnHandle`] that allows access to the result of the spawned future
/// and can cancel it if it is no longer necessary.
///
/// Spawning the future behaves similarly to the [`spawn`] function, but with
/// the addition of returning a `SpawnHandle`. A `SpawnHandle` is itself a
/// future which will eventually complete with the value returned by the
/// spawned future.
///
/// If the spawned future is no longer needed, the [`SpawnHandle::cancel`]
/// method will cancel it. Dropping the `SpawnHandle` will *not* cancel the
/// spawned future, but will result in the item returned by the spawned future
/// being discarded.
///
/// # Panics
///
/// This function will panic if the default executor is not set or if spawning
/// onto the default executor returns an error.
///
/// # Examples
///
/// ```rust
/// # extern crate futures;
/// # extern crate tokio_executor;
/// # use tokio_executor::{spawn_handle, SpawnHandle};
/// use futures::Future;
/// use futures::future::lazy;
///
/// # pub fn dox() {
/// let handle: SpawnHandle<&'static str, ()> = spawn_handle(lazy(|| {
///     Ok("hello from the future!")
/// }));
/// assert_eq!(handle.wait().unwrap(), "hello from the future!");
/// # }
/// # pub fn main() {}
/// ```
///
/// [`SpawnHandle`]: struct.SpawnHandle.html
/// [`spawn`]: fn.spawn.html
/// [`SpawnHandle::cancel`]: struct.SpawnHandle.html#method.cancel
pub fn spawn_handle<T>(future: T) -> SpawnHandle<T::Item, T::Error>
where
    T: Future + Send + 'static,
    T::Item: Send,
    T::Error: Send,
{
    let (tx, rx) = oneshot::channel();
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let f = SpawnedWithHandle {
        cancel_rx,
        tx: Some(tx),
        future,
    };
    spawn(f);
    SpawnHandle {
        cancel_tx,
        rx,
    }
}

/// Set the default executor for the duration of the closure
///
/// # Panics
///
/// This function panics if there already is a default executor set.
pub fn with_default<T, F, R>(executor: &mut T, enter: &mut Enter, f: F) -> R
where T: Executor,
      F: FnOnce(&mut Enter) -> R
{
    EXECUTOR.with(|cell| {
        match cell.get() {
            State::Ready(_) | State::Active =>
                panic!("default executor already set for execution context"),
            _ => {}
        }

        // Ensure that the executor is removed from the thread-local context
        // when leaving the scope. This handles cases that involve panicking.
        struct Reset<'a>(&'a Cell<State>);

        impl<'a> Drop for Reset<'a> {
            fn drop(&mut self) {
                self.0.set(State::Empty);
            }
        }

        let _reset = Reset(cell);

        // While scary, this is safe. The function takes a
        // `&mut Executor`, which guarantees that the reference lives for the
        // duration of `with_default`.
        //
        // Because we are always clearing the TLS value at the end of the
        // function, we can cast the reference to 'static which thread-local
        // cells require.
        let executor = unsafe { hide_lt(executor as &mut _ as *mut _) };

        cell.set(State::Ready(executor));

        f(enter)
    })
}

unsafe fn hide_lt<'a>(p: *mut (Executor + 'a)) -> *mut (Executor + 'static) {
    use std::mem;
    mem::transmute(p)
}


// ===== impl SpawnHandle =====

impl<T, E> Future for SpawnHandle<T, E> {
    type Item = T;
    type Error = SpawnHandleError<E>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.rx.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(i)) => Ok(Async::Ready(i?)),
            Err(oneshot::Canceled) => Err(SpawnHandleError::canceled()),
        }
    }
}

impl<T, E> SpawnHandle<T, E> {
    /// Cancel the spawned future.
    pub fn cancel(self) {
        let _ = self.cancel_tx.send(());
    }
}

impl<T: Future> Future for SpawnedWithHandle<T> {
    type Item = ();
    type Error = ();
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // First, check if the task has been canceled.  If `cancel_rx.poll()`
        // returns an error, that indicates that the `SpawnHandle` has been
        // dropped, which is fine.
        if let Ok(Async::Ready(())) = self.cancel_rx.poll() {
            return Ok(Async::Ready(()));
        }
        // Otherwise, poll the wrapped future.
        let (result, retval) = match self.future.poll() {
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(i)) => (Ok(i), Ok(Async::Ready(()))),
            Err(e) => (Err(e), Err(())),
        };
        // If `send` returns an error, that is because the `SpawnHandle` was
        // dropped, canceling the receiver. This is fine.
        let _ = self.tx.take()
            .expect("polled after ready")
            .send(result);
        retval
    }
}

// ===== impl SpawnHandleError =====

impl<E> From<E> for SpawnHandleError<E> {
    fn from(err: E) -> Self {
        SpawnHandleError {
            kind: SpawnHandleErrorKind::Inner(err),
        }
    }
}

impl<E> SpawnHandleError<E> {
    /// Returns true if the error was caused by the spawned future being canceled.
    pub fn is_canceled(&self) -> bool {
        match self.kind {
            SpawnHandleErrorKind::Canceled => true,
            _ => false,
        }
    }

    /// Returns a new `SpawnHandleError` indicating that the spawned future was
    /// canceled.
    pub fn canceled() -> Self {
        SpawnHandleError {
            kind: SpawnHandleErrorKind::Canceled,
        }
    }

    /// Returns true if the error was caused by the spawned future completing
    /// with an error.
    pub fn is_inner(&self) -> bool {
        match self.kind {
            SpawnHandleErrorKind::Inner(_) => true,
            _ => false,
        }
    }

    /// Consumes self, returning the inner error if this error was caused by the
    /// spawned future failing, or `None` if it was not.
    pub fn into_inner(self) -> Option<E> {
        match self.kind {
            SpawnHandleErrorKind::Inner(e) => Some(e),
            _ => None,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::{Executor, DefaultExecutor, with_default};

    #[test]
    fn default_executor_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DefaultExecutor>();
    }

    #[test]
    fn nested_default_executor_status() {
        let mut enter = super::super::enter().unwrap();
        let mut executor = DefaultExecutor::current();

        let result = with_default(&mut executor, &mut enter, |_| {
            DefaultExecutor::current().status()
        });

        assert!(result.err().unwrap().is_shutdown())
    }
}
