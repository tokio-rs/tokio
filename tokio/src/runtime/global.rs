use crate::runtime::current_thread;
use crate::task::JoinHandle;

#[cfg(feature = "rt-full")]
use crate::runtime::thread_pool;

use std::cell::Cell;
use std::future::Future;

#[derive(Clone, Copy)]
enum State {
    // default executor not defined
    Empty,

    // Current-thread executor
    CurrentThread(*const current_thread::Scheduler),

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
pub(crate) fn spawn<T>(future: T) -> JoinHandle<T::Output>
where
    T: Future + Send + 'static,
    T::Output: Send + 'static,
{
    EXECUTOR.with(|current_executor| match current_executor.get() {
        #[cfg(feature = "rt-full")]
        State::ThreadPool(threadpool_ptr) => {
            let thread_pool = unsafe { &*threadpool_ptr };
            thread_pool.spawn(future)
        }
        State::CurrentThread(current_thread_ptr) => {
            let current_thread = unsafe { &*current_thread_ptr };

            // Safety: The `CurrentThread` value set the thread-local (same
            // thread).
            unsafe { current_thread.spawn(future) }
        }
        State::Empty => {
            // Explicit drop of `future` silences the warning that `future` is
            // not used when neither rt-* feature flags are enabled.
            drop(future);
            panic!("must be called from the context of Tokio runtime");
        }
    })
}

pub(super) fn with_current_thread<F, R>(current_thread: &current_thread::Scheduler, f: F) -> R
where
    F: FnOnce() -> R,
{
    with_state(
        State::CurrentThread(current_thread as *const current_thread::Scheduler),
        f,
    )
}

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
