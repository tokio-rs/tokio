use crate::runtime::park::Unpark;
use crate::runtime::thread_pool::slice;
use crate::task::{self, Schedule, Task};

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
    /// The slice::Set itself is tracked by an `Arc`, but this pointer is not
    /// included in the ref count.
    slices: *const slice::Set<P>,
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
            slices: ptr::null(),
        }
    }

    pub(crate) fn schedule(&self, task: Task<Self>) {
        self.slices().schedule(task);
    }

    pub(super) fn unpark(&self) {
        self.unpark.unpark();
    }

    fn slices(&self) -> &slice::Set<P> {
        unsafe { &*self.slices }
    }

    pub(super) fn set_slices_ptr(&mut self, slices: *const slice::Set<P>) {
        self.slices = slices;
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
            let index = self.slices().index_of(self);
            let owned = &mut *self.slices().owned()[index].get();

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
            let index = self.slices().index_of(self);
            let owned = &mut *self.slices().owned()[index].get();

            owned.release_task(task);
        }
    }

    fn schedule(&self, task: Task<Self>) {
        Self::schedule(self, task);
    }
}
