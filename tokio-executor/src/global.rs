use super::{Executor, SpawnError};
use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;

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
        DefaultExecutor { _dummy: () }
    }

    #[inline]
    fn with_current<F: FnOnce(&mut dyn Executor) -> R, R>(f: F) -> Option<R> {
        EXECUTOR.with(
            |current_executor| match current_executor.replace(State::Active) {
                State::Ready(executor_ptr) => {
                    let executor = unsafe { &mut *executor_ptr };
                    let result = f(executor);
                    current_executor.set(State::Ready(executor_ptr));
                    Some(result)
                }
                State::Empty | State::Active => None,
            },
        )
    }
}

#[derive(Clone, Copy)]
enum State {
    // default executor not defined
    Empty,
    // default executor is defined and ready to be used
    Ready(*mut dyn Executor),
    // default executor is currently active (used to detect recursive calls)
    Active,
}

thread_local! {
    /// Thread-local tracking the current executor
    static EXECUTOR: Cell<State> = Cell::new(State::Empty)
}

// ===== impl DefaultExecutor =====

impl super::Executor for DefaultExecutor {
    fn spawn(
        &mut self,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Result<(), SpawnError> {
        DefaultExecutor::with_current(|executor| executor.spawn(future))
            .unwrap_or_else(|| Err(SpawnError::shutdown()))
    }

    fn status(&self) -> Result<(), SpawnError> {
        DefaultExecutor::with_current(|executor| executor.status())
            .unwrap_or_else(|| Err(SpawnError::shutdown()))
    }
}

impl<T> super::TypedExecutor<T> for DefaultExecutor
where
    T: Future<Output = ()> + Send + 'static,
{
    fn spawn(&mut self, future: T) -> Result<(), SpawnError> {
        super::Executor::spawn(self, Box::pin(future))
    }

    fn status(&self) -> Result<(), SpawnError> {
        super::Executor::status(self)
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
/// ```no_run
/// use tokio_executor::spawn;
/// use futures::future::lazy;
///
/// spawn(lazy(|| {
///     println!("running on the default executor");
///     Ok(())
/// }));
/// ```
pub fn spawn<T>(future: T)
where
    T: Future<Output = ()> + Send + 'static,
{
    DefaultExecutor::current().spawn(Box::pin(future)).unwrap()
}

/// Set the default executor for the duration of the closure
///
/// If a default executor is already set, it will be restored when the closure returns or if it
/// panics.
pub fn with_default<T, F, R>(executor: &mut T, f: F) -> R
where
    T: Executor,
    F: FnOnce() -> R,
{
    EXECUTOR.with(|cell| {
        let was = cell.get();

        // Ensure that the executor is removed from the thread-local context
        // when leaving the scope. This handles cases that involve panicking.
        struct Reset<'a>(&'a Cell<State>, State);

        impl<'a> Drop for Reset<'a> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }

        let _reset = Reset(cell, was);

        // While scary, this is safe. The function takes a
        // `&mut Executor`, which guarantees that the reference lives for the
        // duration of `with_default`.
        //
        // Because we are always clearing the TLS value at the end of the
        // function, we can cast the reference to 'static which thread-local
        // cells require.
        let executor = unsafe { hide_lt(executor as &mut _ as *mut _) };

        cell.set(State::Ready(executor));

        f()
    })
}

unsafe fn hide_lt<'a>(p: *mut (dyn Executor + 'a)) -> *mut (dyn Executor + 'static) {
    use std::mem;
    // false positive: https://github.com/rust-lang/rust-clippy/issues/2906
    #[allow(clippy::transmute_ptr_to_ptr)]
    mem::transmute(p)
}

#[cfg(test)]
mod tests {
    use super::{with_default, DefaultExecutor, Executor};

    #[test]
    fn default_executor_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DefaultExecutor>();
    }

    #[test]
    fn nested_default_executor_status() {
        let _enter = super::super::enter().unwrap();
        let mut executor = DefaultExecutor::current();

        let result = with_default(&mut executor, || DefaultExecutor::current().status());

        assert!(result.err().unwrap().is_shutdown())
    }
}
