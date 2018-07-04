use super::{Executor, Enter, SpawnError};

use futures::Future;

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

#[cfg(feature = "unstable-futures")]
use futures2;

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
    // Prevent the handle from moving across threads.
    _p: PhantomData<Rc<()>>,
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
    pub fn current() -> DefaultExecutor {
        DefaultExecutor {
            _p: PhantomData,
        }
    }
}

/// Thread-local tracking the current executor
thread_local!(static EXECUTOR: Cell<Option<*mut Executor>> = Cell::new(None));

// ===== impl DefaultExecutor =====

impl super::Executor for DefaultExecutor {
    fn spawn(&mut self, future: Box<Future<Item = (), Error = ()> + Send>)
        -> Result<(), SpawnError>
    {
        EXECUTOR.with(|current_executor| {
            match current_executor.get() {
                Some(executor) => {
                    let executor = unsafe { &mut *executor };
                    executor.spawn(future)
                }
                None => {
                    Err(SpawnError::shutdown())
                }
            }
        })
    }

    #[cfg(feature = "unstable-futures")]
    fn spawn2(&mut self, future: Box<futures2::Future<Item = (), Error = futures2::Never> + Send>)
             -> Result<(), futures2::executor::SpawnError>
    {
        EXECUTOR.with(|current_executor| {
            match current_executor.get() {
                Some(executor) => {
                    let executor = unsafe { &mut *executor };
                    executor.spawn2(future)
                }
                None => {
                    Err(futures2::executor::SpawnError::shutdown())
                }
            }
        })
    }

    fn status(&self) -> Result<(), SpawnError> {
        EXECUTOR.with(|current_executor| {
            match current_executor.get() {
                Some(executor) => {
                    let executor = unsafe { &mut *executor };
                    executor.status()
                }
                None => {
                    Err(SpawnError::shutdown())
                }
            }
        })
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

/// Like `spawn` but compatible with futures 0.2
#[cfg(feature = "unstable-futures")]
pub fn spawn2<T>(future: T)
    where T: futures2::Future<Item = (), Error = futures2::Never> + Send + 'static,
{
    DefaultExecutor::current().spawn2(Box::new(future))
        .unwrap()
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
        assert!(cell.get().is_none(), "default executor already set for execution context");

        // Ensure that the executor is removed from the thread-local context
        // when leaving the scope. This handles cases that involve panicking.
        struct Reset<'a>(&'a Cell<Option<*mut Executor>>);

        impl<'a> Drop for Reset<'a> {
            fn drop(&mut self) {
                self.0.set(None);
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

        cell.set(Some(executor));

        f(enter)
    })
}

unsafe fn hide_lt<'a>(p: *mut (Executor + 'a)) -> *mut (Executor + 'static) {
    use std::mem;
    mem::transmute(p)
}
