use crate::loom::sync::atomic::AtomicPtr;
use crate::loom::sync::atomic::Ordering::{AcqRel, Acquire, Release};
use crate::runtime::task::{Header, Notified};

use std::marker::PhantomData;
use std::ptr::{self, NonNull};

pub(crate) struct AtomicCell<S: 'static> {
    // task: AtomicPtr<Header>,
    task: std::cell::UnsafeCell<Option<Notified<S>>>,
    _p: PhantomData<S>,
}

unsafe impl<S: 'static> Send for AtomicCell<S> {}
unsafe impl<S: 'static> Sync for AtomicCell<S> {}

impl<S> AtomicCell<S> {
    pub(crate) fn new() -> AtomicCell<S> {
        AtomicCell {
            // task: AtomicPtr::default(),
            task: Default::default(),
            _p: PhantomData,
        }
    }

    /// Should be called from a local context
    pub(crate) fn is_some(&self) -> bool {
        // !self.task.load(Acquire).is_null()
        unsafe { (*self.task.get()).is_some() }
    }

    pub(crate) fn take_local(&self) -> Option<Notified<S>> {
        unsafe { (*self.task.get()).take() }
        /*
        let ptr = self.task.load(Acquire);

        if ptr.is_null() {
            return None;
        }

        if self
            .task
            .compare_exchange(ptr, ptr::null_mut(), AcqRel, Acquire)
            .is_err()
        {
            return None;
        }

        NonNull::new(ptr).map(|ptr| unsafe { Notified::from_raw(ptr) })
        */
    }

    pub(crate) fn swap_local(&self, task: Notified<S>) -> Option<Notified<S>> {
        std::mem::replace(unsafe { &mut (*self.task.get()) }, Some(task))
        /*
        let next = task.into_raw().as_ptr();
        let prev = self.task.load(Acquire);

        if prev.is_null() {
            // Since this method is only called from the only thread that can
            // set the value to !null, it is safe to use a store here.
            self.task.store(next, Release);
            return None;
        }

        if self
            .task
            .compare_exchange(prev, next, Release, Acquire)
            .is_ok()
        {
            // Safety: we already checked !null above
            let prev = unsafe { Notified::from_raw(NonNull::new_unchecked(prev)) };
            return Some(prev);
        }

        // The compare-exchanged failed, but there is no need to try again since
        // this is the only thread that could set the cell to !null.
        self.task.store(next, Release);
        None
        */
    }

    /*
    pub(crate) fn take_remote(&self) -> Option<Notified<S>> {
        let task = self.task.load(Acquire);

        if task.is_null() {
            return None;
        }

        // Try to take it once
        if self
            .task
            .compare_exchange(task, ptr::null_mut(), Acquire, Acquire)
            .is_ok()
        {
            // safety: we checked for null above
            return Some(unsafe { Notified::from_raw(NonNull::new_unchecked(task)) });
        }

        return None;
    }
    */
}
