#[cfg(feature = "rt-current-thread")]
use crate::executor::current_thread;

#[cfg(feature = "rt-full")]
use crate::executor::thread_pool;

use std::cell::Cell;
use std::future::Future;

#[derive(Clone, Copy)]
enum State {
    // default executor not defined
    Empty,

    // default executor is a thread pool instance.
    #[cfg(feature = "rt-full")]
    ThreadPool(*const thread_pool::Spawner),

    // Current-thread executor
    #[cfg(feature = "rt-current-thread")]
    CurrentThread(*const current_thread::Scheduler),
}

thread_local! {
    /// Thread-local tracking the current executor
    static EXECUTOR: Cell<State> = Cell::new(State::Empty)
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
        #[cfg(feature = "rt-full")]
        State::ThreadPool(threadpool_ptr) => {
            let thread_pool = unsafe { &*threadpool_ptr };
            thread_pool.spawn_background(future);
        }
        #[cfg(feature = "rt-current-thread")]
        State::CurrentThread(current_thread_ptr) => {
            let current_thread = unsafe { &*current_thread_ptr };

            // Safety: The `CurrentThread` value set the thread-local (same
            // thread).
            unsafe {
                current_thread.spawn_background(future);
            }
        }
        State::Empty => {
            // Explicit drop of `future` silences the warning that `future` is
            // not used when neither rt-* feature flags are enabled.
            drop(future);
            panic!("must be called from the context of Tokio runtime");
        }
    })
}

#[cfg(feature = "rt-current-thread")]
pub(super) fn with_current_thread<F, R>(current_thread: &current_thread::Scheduler, f: F) -> R
where
    F: FnOnce() -> R,
{
    with_state(
        State::CurrentThread(current_thread as *const current_thread::Scheduler),
        f,
    )
}

#[cfg(feature = "rt-current-thread")]
pub(super) fn current_thread_is_current(current_thread: &current_thread::Scheduler) -> bool {
    EXECUTOR.with(|current_executor| match current_executor.get() {
        State::CurrentThread(ptr) => ptr == current_thread as *const _,
        _ => false,
    })
}

#[cfg(feature = "rt-full")]
pub(super) fn with_thread_pool<F, R>(thread_pool: &thread_pool::Spawner, f: F) -> R
where
    F: FnOnce() -> R,
{
    with_state(State::ThreadPool(thread_pool as *const _), f)
}

#[cfg(feature = "rt-current-thread")]
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

        cell.set(state);

        f()
    })
}
