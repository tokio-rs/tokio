//! Asynchronous green-threads.

cfg_blocking! {
    mod blocking;
    pub use blocking::spawn_blocking;

    cfg_rt_threaded! {
        pub use blocking::block_in_place;
    }
}

mod core;
use self::core::Cell;
pub(crate) use self::core::Header;

mod error;
pub use self::error::JoinError;

mod harness;
use self::harness::Harness;

cfg_rt_core! {
    mod join;
    #[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
    pub use self::join::JoinHandle;
}

mod list;
pub(crate) use self::list::OwnedList;

mod raw;
use self::raw::RawTask;

cfg_rt_core! {
    mod spawn;
    pub use spawn::spawn;
}

mod stack;
pub(crate) use self::stack::TransferStack;

mod state;
use self::state::{Snapshot, State};

mod waker;

mod yield_now;
pub use yield_now::yield_now;

/// Unit tests
#[cfg(test)]
mod tests;

use std::future::Future;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::{fmt, mem};

/// An owned handle to the task, tracked by ref count
pub(crate) struct Task<S: 'static> {
    raw: RawTask,
    _p: PhantomData<S>,
}

unsafe impl<S: Send + Sync + 'static> Send for Task<S> {}

/// Task result sent back
pub(crate) type Result<T> = std::result::Result<T, JoinError>;

pub(crate) trait Schedule: Send + Sync + Sized + 'static {
    /// Bind a task to the executor.
    ///
    /// Guaranteed to be called from the thread that called `poll` on the task.
    fn bind(&self, task: &Task<Self>);

    /// The task has completed work and is ready to be released. The scheduler
    /// is free to drop it whenever.
    fn release(&self, task: Task<Self>);

    /// The has been completed by the executor it was bound to.
    fn release_local(&self, task: &Task<Self>);

    /// Schedule the task
    fn schedule(&self, task: Task<Self>);
}

cfg_rt_threaded! {
    /// Create a new task without an associated join handle
    pub(crate) fn background<T, S>(task: T) -> Task<S>
    where
        T: Future + Send + 'static,
        S: Schedule,
    {
        Task {
            raw: RawTask::new_background::<_, S>(task),
            _p: PhantomData,
        }
    }
}

/// Create a new task with an associated join handle
pub(crate) fn joinable<T, S>(task: T) -> (Task<S>, JoinHandle<T::Output>)
where
    T: Future + Send + 'static,
    S: Schedule,
{
    let raw = RawTask::new_joinable::<_, S>(task);

    let task = Task {
        raw,
        _p: PhantomData,
    };

    let join = JoinHandle::new(raw);

    (task, join)
}

impl<S: 'static> Task<S> {
    pub(crate) unsafe fn from_raw(ptr: NonNull<Header>) -> Task<S> {
        Task {
            raw: RawTask::from_raw(ptr),
            _p: PhantomData,
        }
    }

    pub(crate) fn header(&self) -> &Header {
        self.raw.header()
    }

    pub(crate) fn into_raw(self) -> NonNull<Header> {
        let raw = self.raw.into_raw();
        mem::forget(self);
        raw
    }
}

impl<S: Schedule> Task<S> {
    /// Returns `self` when the task needs to be immediately re-scheduled
    pub(crate) fn run<F>(self, mut executor: F) -> Option<Self>
    where
        F: FnMut() -> Option<NonNull<S>>,
    {
        if unsafe {
            self.raw
                .poll(&mut || executor().map(|ptr| ptr.cast::<()>()))
        } {
            Some(self)
        } else {
            // Cleaning up the `Task` instance is done from within the poll
            // function.
            mem::forget(self);
            None
        }
    }

    /// Pre-emptively cancel the task as part of the shutdown process.
    pub(crate) fn shutdown(self) {
        self.raw.cancel_from_queue();
        mem::forget(self);
    }
}

impl<S: 'static> Drop for Task<S> {
    fn drop(&mut self) {
        self.raw.drop_task();
    }
}

impl<S> fmt::Debug for Task<S> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Task").finish()
    }
}
