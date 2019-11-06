use crate::loom::alloc::Track;
use crate::runtime::task::Cell;
use crate::runtime::task::Harness;
use crate::runtime::task::{Header, Schedule};
use crate::runtime::task::{Snapshot, State};

use std::future::Future;
use std::ptr::NonNull;
use std::task::Waker;

/// Raw task handle
pub(super) struct RawTask {
    ptr: NonNull<Header>,
}

pub(super) struct Vtable {
    /// Poll the future
    pub(super) poll: unsafe fn(*mut (), &mut dyn FnMut() -> Option<NonNull<()>>) -> bool,

    /// The task handle has been dropped and the join waker needs to be dropped
    /// or the task struct needs to be deallocated
    pub(super) drop_task: unsafe fn(*mut ()),

    /// Read the task output
    pub(super) read_output: unsafe fn(*mut (), *mut (), Snapshot),

    /// Store the join handle's waker
    ///
    /// Returns a snapshot of the state **after** the transition
    pub(super) store_join_waker: unsafe fn(*mut (), &Waker) -> Snapshot,

    /// Replace the join handle's waker
    ///
    /// Returns a snapshot of the state **after** the transition
    pub(super) swap_join_waker: unsafe fn(*mut (), &Waker, Snapshot) -> Snapshot,

    /// The join handle has been dropped
    pub(super) drop_join_handle_slow: unsafe fn(*mut ()),

    /// The task is being canceled
    pub(super) cancel: unsafe fn(*mut (), bool),
}

/// Get the vtable for the requested `T` and `S` generics.
pub(super) fn vtable<T: Future, S: Schedule>() -> &'static Vtable {
    &Vtable {
        poll: poll::<T, S>,
        drop_task: drop_task::<T, S>,
        read_output: read_output::<T, S>,
        store_join_waker: store_join_waker::<T, S>,
        swap_join_waker: swap_join_waker::<T, S>,
        drop_join_handle_slow: drop_join_handle_slow::<T, S>,
        cancel: cancel::<T, S>,
    }
}

impl RawTask {
    pub(super) fn new_background<T, S>(task: T) -> RawTask
    where
        T: Future + Send + 'static,
        S: Schedule,
    {
        RawTask::new::<_, S>(task, State::new_background())
    }

    pub(super) fn new_joinable<T, S>(task: T) -> RawTask
    where
        T: Future + Send + 'static,
        S: Schedule,
    {
        RawTask::new::<_, S>(task, State::new_joinable())
    }

    fn new<T, S>(task: T, state: State) -> RawTask
    where
        T: Future + Send + 'static,
        S: Schedule,
    {
        let ptr = Box::into_raw(Cell::new::<S>(task, state));
        let ptr = unsafe { NonNull::new_unchecked(ptr as *mut Header) };

        RawTask { ptr }
    }

    pub(super) unsafe fn from_raw(ptr: NonNull<Header>) -> RawTask {
        RawTask { ptr }
    }

    /// Returns a reference to the task's meta structure.
    ///
    /// Safe as `Header` is `Sync`.
    pub(super) fn header(&self) -> &Header {
        unsafe { self.ptr.as_ref() }
    }

    /// Returns a raw pointer to the task's meta structure.
    pub(super) fn into_raw(self) -> NonNull<Header> {
        self.ptr
    }

    /// Safety: mutual exclusion is required to call this function.
    ///
    /// Returns `true` if the task needs to be scheduled again.
    pub(super) unsafe fn poll(self, executor: &mut dyn FnMut() -> Option<NonNull<()>>) -> bool {
        // Get the vtable without holding a ref to the meta struct. This is done
        // because a mutable reference to the task is passed into the poll fn.
        let vtable = self.header().vtable;

        (vtable.poll)(self.ptr.as_ptr() as *mut (), executor)
    }

    pub(super) fn drop_task(self) {
        let vtable = self.header().vtable;
        unsafe {
            (vtable.drop_task)(self.ptr.as_ptr() as *mut ());
        }
    }

    pub(super) unsafe fn read_output(self, dst: *mut (), state: Snapshot) {
        let vtable = self.header().vtable;
        (vtable.read_output)(self.ptr.as_ptr() as *mut (), dst, state);
    }

    pub(super) fn store_join_waker(self, waker: &Waker) -> Snapshot {
        let vtable = self.header().vtable;
        unsafe { (vtable.store_join_waker)(self.ptr.as_ptr() as *mut (), waker) }
    }

    pub(super) fn swap_join_waker(self, waker: &Waker, prev: Snapshot) -> Snapshot {
        let vtable = self.header().vtable;
        unsafe { (vtable.swap_join_waker)(self.ptr.as_ptr() as *mut (), waker, prev) }
    }

    pub(super) fn drop_join_handle_slow(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.drop_join_handle_slow)(self.ptr.as_ptr() as *mut ()) }
    }

    pub(super) fn cancel_from_queue(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.cancel)(self.ptr.as_ptr() as *mut (), true) }
    }
}

impl Clone for RawTask {
    fn clone(&self) -> Self {
        RawTask { ptr: self.ptr }
    }
}

impl Copy for RawTask {}

unsafe fn poll<T: Future, S: Schedule>(
    ptr: *mut (),
    executor: &mut dyn FnMut() -> Option<NonNull<()>>,
) -> bool {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.poll(executor)
}

unsafe fn drop_task<T: Future, S: Schedule>(ptr: *mut ()) {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.drop_task();
}

unsafe fn read_output<T: Future, S: Schedule>(ptr: *mut (), dst: *mut (), state: Snapshot) {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.read_output(dst as *mut Track<super::Result<T::Output>>, state);
}

unsafe fn store_join_waker<T: Future, S: Schedule>(ptr: *mut (), waker: &Waker) -> Snapshot {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.store_join_waker(waker)
}

unsafe fn swap_join_waker<T: Future, S: Schedule>(
    ptr: *mut (),
    waker: &Waker,
    prev: Snapshot,
) -> Snapshot {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.swap_join_waker(waker, prev)
}

unsafe fn drop_join_handle_slow<T: Future, S: Schedule>(ptr: *mut ()) {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.drop_join_handle_slow()
}

unsafe fn cancel<T: Future, S: Schedule>(ptr: *mut (), from_queue: bool) {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.cancel(from_queue)
}
