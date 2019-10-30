use crate::executor::task::{Header, Task};

use std::fmt;
use std::ptr::NonNull;

pub(crate) struct OwnedList<T: 'static> {
    head: Option<NonNull<Header<T>>>,
}

impl<T: 'static> OwnedList<T> {
    pub(crate) fn new() -> OwnedList<T> {
        OwnedList { head: None }
    }

    pub(crate) fn insert(&mut self, task: &Task<T>) {
        unsafe {
            debug_assert!((*task.header().owned_next.get()).is_none());
            debug_assert!((*task.header().owned_prev.get()).is_none());

            let ptr = Some(task.header().into());

            if let Some(next) = self.head {
                debug_assert!((*next.as_ref().owned_prev.get()).is_none());
                *next.as_ref().owned_prev.get() = ptr;
            }

            *task.header().owned_next.get() = self.head;
            self.head = ptr;
        }
    }

    pub(crate) fn remove(&mut self, task: &Task<T>) {
        unsafe {
            if let Some(next) = *task.header().owned_next.get() {
                *next.as_ref().owned_prev.get() = *task.header().owned_prev.get();
            }

            if let Some(prev) = *task.header().owned_prev.get() {
                *prev.as_ref().owned_next.get() = *task.header().owned_next.get();
            } else {
                debug_assert_eq!(self.head, Some(task.header().into()));
                self.head = *task.header().owned_next.get();
            }
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// Transition all tasks in the list to canceled as part of the shutdown
    /// process.
    pub(crate) fn shutdown(&self) {
        let mut curr = self.head;

        while let Some(task) = curr {
            unsafe {
                let vtable = task.as_ref().vtable;
                (vtable.cancel)(task.as_ptr() as *mut (), false);
                curr = *task.as_ref().owned_next.get();
            }
        }
    }
}

impl<T: 'static> fmt::Debug for OwnedList<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("OwnedList").finish()
    }
}
