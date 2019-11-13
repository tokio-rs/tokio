use crate::runtime::local;

#[cfg(feature = "rt-full")]
use crate::runtime::thread_pool;

use std::cell::Cell;
use std::future::Future;

#[derive(Clone, Copy)]
enum State {
    // default executor not defined
    Empty,

    // Local (to the thread) scheduler
    Local(*const local::SchedulerInner),

    // default executor is a thread pool instance.
    #[cfg(feature = "rt-full")]
    ThreadPool(*const thread_pool::Spawner),
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
        State::ThreadPool(thread_pool_ptr) => {
            let thread_pool = unsafe { &*thread_pool_ptr };
            thread_pool.spawn_background(future);
        }
        State::Local(local_scheduler_ptr) => {
            let local_scheduler = unsafe { &*local_scheduler_ptr };

            // Safety: The `LocalScheduler` value set the thread-local (same
            // thread).
            unsafe {
                local_scheduler.spawn_background(future);
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

pub(super) fn with_local_scheduler<F, R>(local_scheduler: &local::SchedulerInner, f: F) -> R
where
    F: FnOnce() -> R,
{
    with_state(
        State::Local(local_scheduler as *const local::SchedulerInner),
        f,
    )
}

pub(super) fn local_scheduler_is_current(local_scheduler: &local::SchedulerInner) -> bool {
    EXECUTOR.with(|current_executor| match current_executor.get() {
        State::Local(ptr) => ptr == local_scheduler as *const _,
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
