mod core;
use self::core::Cell;
use self::core::Header;

mod error;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::error::JoinError;

mod harness;
use self::harness::Harness;

cfg_rt_multi_thread! {
    mod inject;
    pub(super) use self::inject::Inject;
}

mod join;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::join::JoinHandle;

mod list;
pub(crate) use self::list::{LocalOwnedTasks, OwnedTasks};

mod raw;
use self::raw::RawTask;

mod state;
use self::state::State;

mod waker;

use crate::future::Future;
use crate::util::linked_list;

use std::marker::PhantomData;
use std::ptr::NonNull;
use std::{fmt, mem};

/// An owned handle to the task, tracked by ref count
#[repr(transparent)]
pub(crate) struct Task<S: 'static> {
    raw: RawTask,
    _p: PhantomData<S>,
}

unsafe impl<S> Send for Task<S> {}
unsafe impl<S> Sync for Task<S> {}

/// A task was notified.
#[repr(transparent)]
pub(crate) struct Notified<S: 'static>(Task<S>);

// safety: This type cannot be used to touch the task without first verifying
// that the value is on a thread where it is safe to poll the task.
unsafe impl<S: Schedule> Send for Notified<S> {}
unsafe impl<S: Schedule> Sync for Notified<S> {}

/// A non-Send variant of Notified with the invariant that it is on a thread
/// where it is safe to poll it.
#[repr(transparent)]
pub(crate) struct LocalNotified<S: 'static> {
    task: Task<S>,
    _not_send: PhantomData<*const ()>,
}

/// A task that is not owned by any OwnedTasks. Used for blocking tasks.
/// This type holds two ref-counts.
pub(crate) struct UnownedTask<S: 'static> {
    raw: RawTask,
    _p: PhantomData<S>,
}

// safety: This type can only be created given a Send task.
unsafe impl<S> Send for UnownedTask<S> {}
unsafe impl<S> Sync for UnownedTask<S> {}

/// Task result sent back
pub(crate) type Result<T> = std::result::Result<T, JoinError>;

pub(crate) trait Schedule: Sync + Sized + 'static {
    /// The task has completed work and is ready to be released. The scheduler
    /// should release it immediately and return it. The task module will batch
    /// the ref-dec with setting other options.
    ///
    /// If the scheduler has already released the task, then None is returned.
    fn release(&self, task: &Task<Self>) -> Option<Task<Self>>;

    /// Schedule the task
    fn schedule(&self, task: Notified<Self>);

    /// Schedule the task to run in the near future, yielding the thread to
    /// other tasks.
    fn yield_now(&self, task: Notified<Self>) {
        self.schedule(task);
    }
}

cfg_rt! {
    /// This is the constructor for a new task. Three references to the task are
    /// created. The first task reference is usually put into an OwnedTasks
    /// immediately. The Notified is sent to the scheduler as an ordinary
    /// notification.
    fn new_task<T, S>(
        task: T,
        scheduler: S
    ) -> (Task<S>, Notified<S>, JoinHandle<T::Output>)
    where
        S: Schedule,
        T: Future + 'static,
        T::Output: 'static,
    {
        let raw = RawTask::new::<T, S>(task, scheduler);
        let task = Task {
            raw,
            _p: PhantomData,
        };
        let notified = Notified(Task {
            raw,
            _p: PhantomData,
        });
        let join = JoinHandle::new(raw);

        (task, notified, join)
    }

    /// Create a new task with an associated join handle. This method is used
    /// only when the task is not going to be stored in an `OwnedTasks` list.
    ///
    /// Currently only blocking tasks use this method.
    pub(crate) fn unowned<T, S>(task: T, scheduler: S) -> (UnownedTask<S>, JoinHandle<T::Output>)
    where
        S: Schedule,
        T: Send + Future + 'static,
        T::Output: Send + 'static,
    {
        let (task, notified, join) = new_task(task, scheduler);

        // This transfers the ref-count of task and notified into an UnownedTask.
        // This is valid because an UnownedTask holds two ref-counts.
        let unowned = UnownedTask {
            raw: task.raw,
            _p: PhantomData,
        };
        std::mem::forget(task);
        std::mem::forget(notified);

        (unowned, join)
    }
}

impl<S: 'static> Task<S> {
    unsafe fn from_raw(ptr: NonNull<Header>) -> Task<S> {
        Task {
            raw: RawTask::from_raw(ptr),
            _p: PhantomData,
        }
    }

    fn header(&self) -> &Header {
        self.raw.header()
    }
}

cfg_rt_multi_thread! {
    impl<S: 'static> Notified<S> {
        unsafe fn from_raw(ptr: NonNull<Header>) -> Notified<S> {
            Notified(Task::from_raw(ptr))
        }
    }

    impl<S: 'static> Task<S> {
        fn into_raw(self) -> NonNull<Header> {
            let ret = self.header().into();
            mem::forget(self);
            ret
        }
    }

    impl<S: 'static> Notified<S> {
        fn into_raw(self) -> NonNull<Header> {
            self.0.into_raw()
        }
    }
}

impl<S: Schedule> Task<S> {
    /// Pre-emptively cancel the task as part of the shutdown process.
    pub(crate) fn shutdown(&self) {
        self.raw.shutdown();
    }
}

impl<S: Schedule> LocalNotified<S> {
    /// Run the task
    pub(crate) fn run(self) {
        self.task.raw.poll();
        mem::forget(self);
    }
}

impl<S: Schedule> UnownedTask<S> {
    // Used in test of the inject queue.
    #[cfg(test)]
    pub(super) fn into_notified(self) -> Notified<S> {
        Notified(self.into_task())
    }

    fn into_task(self) -> Task<S> {
        // Convert into a task.
        let task = Task {
            raw: self.raw,
            _p: PhantomData,
        };
        mem::forget(self);

        // Drop a ref-count since an UnownedTask holds two.
        task.header().state.ref_dec();

        task
    }

    pub(crate) fn run(self) {
        // Decrement the ref-count
        self.raw.header().state.ref_dec();
        // Poll the task
        self.raw.poll();
        mem::forget(self);
    }

    pub(crate) fn shutdown(self) {
        self.into_task().shutdown()
    }
}

impl<S: 'static> Drop for Task<S> {
    fn drop(&mut self) {
        // Decrement the ref count
        if self.header().state.ref_dec() {
            // Deallocate if this is the final ref count
            self.raw.dealloc();
        }
    }
}

impl<S: 'static> Drop for UnownedTask<S> {
    fn drop(&mut self) {
        // Decrement the ref count
        if self.raw.header().state.ref_dec_twice() {
            // Deallocate if this is the final ref count
            self.raw.dealloc();
        }
    }
}

impl<S> fmt::Debug for Task<S> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "Task({:p})", self.header())
    }
}

impl<S> fmt::Debug for Notified<S> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "task::Notified({:p})", self.0.header())
    }
}

/// # Safety
///
/// Tasks are pinned
unsafe impl<S> linked_list::Link for Task<S> {
    type Handle = Task<S>;
    type Target = Header;

    fn as_raw(handle: &Task<S>) -> NonNull<Header> {
        handle.header().into()
    }

    unsafe fn from_raw(ptr: NonNull<Header>) -> Task<S> {
        Task::from_raw(ptr)
    }

    unsafe fn pointers(target: NonNull<Header>) -> NonNull<linked_list::Pointers<Header>> {
        // Not super great as it avoids some of looms checking...
        NonNull::from(target.as_ref().owned.with_mut(|ptr| &mut *ptr))
    }
}
