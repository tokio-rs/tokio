use crate::loom::cell::CausalCell;
use crate::task::{Snapshot, State};

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr::NonNull;
use std::task::Waker;

pub(crate) struct Header<S: 'static> {
    /// Task state
    pub(super) state: State,

    /// Pointer to the executor owned by the task
    pub(super) executor: CausalCell<Option<NonNull<S>>>,

    /// Pointer to next task, used for misc task linked lists.
    pub(crate) queue_next: UnsafeCell<*const Header<S>>,

    /// Pointer to the next task in the ownership list.
    pub(crate) owned_next: UnsafeCell<*const Header<S>>,

    /// Pointer to the previous task in the ownership list.
    pub(crate) owned_prev: UnsafeCell<*const Header<S>>,

    /// Table of function pointers for executing actions on the task.
    pub(super) vtable: &'static Vtable<S>,

    /// Used by loom to track the causality of the future. Without loom, this is
    /// unit.
    pub(super) future_causality: CausalCell<()>,
}

pub(super) struct Trailer {
    /// Consumer task waiting on completion of this task.
    pub(super) waker: CausalCell<MaybeUninit<Option<Waker>>>,
}

pub(super) struct Vtable<S: 'static> {
    /// Poll the future
    pub(super) poll: unsafe fn(*mut (), NonNull<S>) -> bool,

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

impl<S> Header<S> {
    pub(super) fn executor(&self) -> Option<NonNull<S>> {
        unsafe { self.executor.with(|ptr| *ptr) }
    }
}
