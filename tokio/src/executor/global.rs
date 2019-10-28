#[cfg(feature = "rt-full")]
use crate::executor::thread_pool::ThreadPool;
use crate::executor::{Executor, SpawnError};

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
/// variable and is set using `tokio::executor::with_default`
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
        EXECUTOR.with(|current_executor| match current_executor.get() {
            State::Ready(executor_ptr) => {
                let executor = unsafe { &mut *executor_ptr };
                Some(f(executor))
            }
            #[cfg(feature = "rt-full")]
            State::ThreadPool(threadpool_ptr) => {
                let mut thread_pool = unsafe { &*threadpool_ptr };
                Some(f(&mut thread_pool))
            }
            State::Empty => None,
        })
    }
}

#[derive(Clone, Copy)]
enum State {
    // default executor not defined
    Empty,

    // default executor is a thread pool instance.
    #[cfg(feature = "rt-full")]
    ThreadPool(*const ThreadPool),

    // default executor is set to a custom executor.
    Ready(*mut dyn Executor),
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

/// Spawns a future on the default executor.
///
/// In order for a future to do work, it must be spawned on an executor. The
/// `spawn` function is the easiest way to do this. It spawns a future on the
/// [default executor] for the current execution context (tracked using a
/// thread-local variable).
///
/// The default executor is **usually** a thread pool.
///
/// # Examples
///
/// In this example, a server is started and `spawn` is used to start a new task
/// that processes each received connection.
///
/// ```
/// use tokio::net::TcpListener;
///
/// # async fn process<T>(_t: T) {}
/// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
/// let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
///
/// loop {
///     let (socket, _) = listener.accept().await?;
///
///     tokio::spawn(async move {
///         // Process each socket concurrently.
///         process(socket).await
///     });
/// }
/// # }
/// ```
///
/// [default executor]: struct.DefaultExecutor.html
///
/// # Panics
///
/// This function will panic if the default executor is not set or if spawning
/// onto the default executor returns an error. To avoid the panic, use
/// [`DefaultExecutor`].
///
/// [`DefaultExecutor`]: struct.DefaultExecutor.html
pub fn spawn<T>(future: T)
where
    T: Future<Output = ()> + Send + 'static,
{
    EXECUTOR.with(|current_executor| match current_executor.get() {
        State::Ready(executor_ptr) => {
            let executor = unsafe { &mut *executor_ptr };
            executor.spawn(Box::pin(future)).unwrap();
        }
        #[cfg(feature = "rt-full")]
        State::ThreadPool(threadpool_ptr) => {
            let thread_pool = unsafe { &*threadpool_ptr };
            thread_pool.spawn_background(future);
        }
        State::Empty => panic!("must be called from the context of Tokio runtime"),
    })
}

#[cfg(feature = "rt-full")]
pub(crate) fn with_threadpool<F, R>(thread_pool: &ThreadPool, f: F) -> R
where
    F: FnOnce() -> R,
{
    with_state(State::ThreadPool(thread_pool as *const ThreadPool), f)
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
    // While scary, this is safe. The function takes a
    // `&mut Executor`, which guarantees that the reference lives for the
    // duration of `with_default`.
    //
    // Because we are always clearing the TLS value at the end of the
    // function, we can cast the reference to 'static which thread-local
    // cells require.
    let executor = unsafe { hide_lt(executor as &mut _ as *mut _) };
    with_state(State::Ready(executor), f)
}

fn with_state<F, R>(state: State, f: F) -> R
where
    F: FnOnce() -> R,
{
    EXECUTOR.with(|cell| {
        let was = cell.replace(State::Empty);

        // Ensure that the executor is removed from the thread-local context
        // when leaving the scope. This handles cases that involve panicking.
        struct Reset<'a>(&'a Cell<State>, State);

        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }

        let _reset = Reset(cell, was);

        if let State::Ready(executor) = state {
            let executor = unsafe { &mut *executor };

            if executor.status().is_err() {
                panic!("executor not active; is this because `with_default` is called with `DefaultExecutor`?");
            }
        }

        cell.set(state);

        f()
    })
}

unsafe fn hide_lt<'a>(p: *mut (dyn Executor + 'a)) -> *mut (dyn Executor + 'static) {
    use std::mem;
    // false positive: https://github.com/rust-lang/rust-clippy/issues/2906
    #[allow(clippy::transmute_ptr_to_ptr)]
    mem::transmute(p)
}
