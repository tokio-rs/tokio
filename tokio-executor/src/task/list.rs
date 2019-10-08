use crate::task::{Header, Task};

use std::fmt;
use std::ptr;

pub(crate) struct OwnedList<T: 'static> {
    head: *const Header<T>,
}

impl<T: 'static> OwnedList<T> {
    pub(crate) fn new() -> OwnedList<T> {
        OwnedList { head: ptr::null() }
    }

    pub(crate) fn insert(&mut self, task: &Task<T>) {
        unsafe {
            debug_assert!((*task.header().owned_next.get()).is_null());
            debug_assert!((*task.header().owned_prev.get()).is_null());

            let ptr = task.header() as *const _;

            if let Some(next) = self.head.as_ref() {
                debug_assert!((*next.owned_prev.get()).is_null());
                *next.owned_prev.get() = ptr;
            }

            *task.header().owned_next.get() = self.head;
            self.head = ptr;
        }
    }

    pub(crate) fn remove(&mut self, task: &Task<T>) {
        debug_assert!(self.contains(task));

        unsafe {
            if let Some(next) = (*task.header().owned_next.get()).as_ref() {
                *next.owned_prev.get() = *task.header().owned_prev.get();
            }

            if let Some(prev) = (*task.header().owned_prev.get()).as_ref() {
                *prev.owned_next.get() = *task.header().owned_next.get();
            } else {
                debug_assert_eq!(self.head, task.header() as *const _);
                self.head = *task.header().owned_next.get();
            }
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.head.is_null()
    }

    /// Transition all tasks in the list to canceled as part of the shutdown
    /// process.
    pub(crate) fn shutdown(&self) {
        let mut curr = self.head;

        while !curr.is_null() {
            unsafe {
                let vtable = (*curr).vtable;
                (vtable.cancel)(curr as *mut (), false);
                curr = *(*curr).owned_next.get();
            }
        }
    }

    fn contains(&self, task: &Task<T>) -> bool {
        let mut curr = self.head;

        unsafe {
            while let Some(h) = curr.as_ref() {
                if h as *const _ == task.header() as *const _ {
                    return true;
                }

                curr = *h.owned_next.get();
            }
        }

        false
    }
}

impl<T: 'static> fmt::Debug for OwnedList<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("OwnedList").finish()
    }
}
