mod core;
pub(crate) use self::core::Header;

mod error;
pub use self::error::Error;

mod harness;

mod join;
pub(crate) use self::join::JoinHandle;

mod list;
pub(crate) use self::list::OwnedList;

mod raw;

mod stack;
pub(crate) use self::stack::TransferStack;

mod state;
mod waker;

/// Unit tests
#[cfg(test)]
mod tests;

use self::raw::RawTask;

use std::future::Future;
use std::ptr::NonNull;
use std::{fmt, mem};

/// An owned handle to the task, tracked by ref count
pub(crate) struct Task<S: 'static> {
    raw: RawTask<S>,
}

unsafe impl<S: Send + Sync + 'static> Send for Task<S> {}

/// Task result sent back
pub(crate) type Result<T> = std::result::Result<T, Error>;

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

/// Create a new task without an associated join handle
pub(crate) fn background<T, S>(task: T) -> Task<S>
where
    T: Future + Send + 'static,
    S: Schedule,
{
    let raw = RawTask::new_background(task);
    Task { raw }
}

/// Create a new task with an associated join handle
pub(crate) fn joinable<T, S>(task: T) -> (Task<S>, JoinHandle<T::Output, S>)
where
    T: Future + Send + 'static,
    S: Schedule,
{
    let raw = RawTask::new_joinable(task);
    let task = Task { raw };
    let join = JoinHandle::new(raw);

    (task, join)
}

impl<S: 'static> Task<S> {
    pub(crate) unsafe fn from_raw(ptr: NonNull<Header<S>>) -> Task<S> {
        let raw = RawTask::from_raw(ptr);
        Task { raw }
    }

    pub(crate) fn header(&self) -> &Header<S> {
        self.raw.header()
    }

    pub(crate) fn into_raw(self) -> NonNull<Header<S>> {
        let raw = self.raw.into_raw();
        mem::forget(self);
        raw
    }
}

impl<S: Schedule> Task<S> {
    /// Returns `self` when the task needs to be immediately re-scheduled
    pub(crate) fn run(self, executor: &mut dyn FnMut() -> Option<NonNull<S>>) -> Option<Self> {
        if unsafe { self.raw.poll(executor) } {
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
