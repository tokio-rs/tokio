use super::{Executor, Enter, SpawnError};

use futures::Future;

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

/// Executes futures on the currently configured global executor.
#[derive(Debug, Clone)]
pub struct DefaultExecutor {
    // Prevent the handle from moving across threads.
    _p: PhantomData<Rc<()>>,
}

impl DefaultExecutor {
    /// Returns a handle to the default executor for the current context.
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
}

// ===== global spawn fns =====

/// Submits a future for execution on the global executor -- usually a
/// threadpool.
pub fn spawn<T>(future: T)
    where T: Future<Item = (), Error = ()> + Send + 'static,
{
    DefaultExecutor::current().spawn(Box::new(future))
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
