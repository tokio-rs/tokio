use crate::runtime::park::Unpark;
use crate::runtime::task::{self, Schedule, Task};
use crate::runtime::thread_pool::worker;

use std::ptr;

/// Per-worker data accessible from any thread.
///
/// Accessed by:
///
/// - other workers
/// - tasks
///
pub(crate) struct Shared<P>
where
    P: 'static,
{
    /// Thread unparker
    unpark: P,

    /// Tasks pending drop. Any worker pushes tasks, only the "owning" worker
    /// pops.
    pub(super) pending_drop: task::TransferStack<Self>,

    /// Untracked pointer to the pool.
    ///
    /// The pool itself is tracked by an `Arc`, but this pointer is not included
    /// in the ref count.
    ///
    /// # Safety
    ///
    /// `Worker` instances are stored in the `Pool` and are never removed.
    set: *const worker::Set<P>,
}

unsafe impl<P: Unpark> Send for Shared<P> {}
unsafe impl<P: Unpark> Sync for Shared<P> {}

impl<P> Shared<P>
where
    P: Unpark,
{
    pub(super) fn new(unpark: P) -> Shared<P> {
        Shared {
            unpark,
            pending_drop: task::TransferStack::new(),
            set: ptr::null(),
        }
    }

    pub(crate) fn schedule(&self, task: Task<Self>) {
        self.set().schedule(task);
    }

    pub(super) fn unpark(&self) {
        self.unpark.unpark();
    }

    pub(super) fn set_container_ptr(&mut self, set: *const worker::Set<P>) {
        self.set = set;
    }

    fn set(&self) -> &worker::Set<P> {
        unsafe { &*self.set }
    }
}

impl<P> Schedule for Shared<P>
where
    P: Unpark,
{
    fn bind(&self, task: &Task<Self>) {
        // Get access to the Owned component. This function can only be called
        // when on the worker.
        unsafe {
            let index = self.set().index_of(self);
            let owned = &mut *self.set().owned()[index].get();

            owned.bind_task(task);
        }
    }

    fn release(&self, task: Task<Self>) {
        // This stores the task with the owning worker. The worker is not
        // notified. Instead, the worker will clean up the tasks "eventually".
        //
        self.pending_drop.push(task);
    }

    fn release_local(&self, task: &Task<Self>) {
        // Get access to the Owned component. This function can only be called
        // when on the worker.
        unsafe {
            let index = self.set().index_of(self);
            let owned = &mut *self.set().owned()[index].get();

            owned.release_task(task);
        }
    }

    fn schedule(&self, task: Task<Self>) {
        Self::schedule(self, task);
    }
}
