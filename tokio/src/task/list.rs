use crate::task::{self, Header, Task};

use std::fmt;
use std::marker::PhantomData;
use std::ptr::NonNull;

pub(crate) struct OwnedList<T: 'static> {
    head: Option<NonNull<Header>>,
    _p: PhantomData<T>,
}

unsafe impl<T: task::ScheduleSendOnly + 'static> Send for OwnedList<T> {}
unsafe impl<T: task::ScheduleSendOnly + 'static> Sync for OwnedList<T> {}

impl<T: task::Schedule> OwnedList<T> {
    pub(crate) fn new() -> OwnedList<T> {
        OwnedList {
            head: None,
            _p: PhantomData,
        }
    }

    pub(crate) fn insert(&mut self, task: &Task<T>) {
        debug_assert!(!self.contains(task));

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
            if !self.in_list(task) {
                // The task is not in the list
                return;
            }

            assert!(self.head.is_some());

            if let Some(next) = *task.header().owned_next.get() {
                *next.as_ref().owned_prev.get() = *task.header().owned_prev.get();
            }

            if let Some(prev) = *task.header().owned_prev.get() {
                *prev.as_ref().owned_next.get() = *task.header().owned_next.get();
            } else {
                assert_eq!(self.head, Some(task.header().into()));
                self.head = *task.header().owned_next.get();
            }

            *task.header().owned_next.get() = None;
            *task.header().owned_prev.get() = None;
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// Pop a task
    pub(crate) fn pop(&mut self) -> Option<Task<T>> {
        let head = match self.head {
            Some(head) => head,
            None => return None,
        };

        let task = unsafe { Task::<T>::from_raw(head) };
        self.remove(&task);
        Some(task)
    }

    /// Only used by debug assertions
    fn contains(&self, task: &Task<T>) -> bool {
        let mut curr = self.head;

        while let Some(p) = curr {
            if p == task.header().into() {
                return true;
            }

            unsafe {
                curr = *p.as_ref().owned_next.get();
            }
        }

        false
    }

    fn in_list(&self, task: &Task<T>) -> bool {
        self.head == Some(task.header().into())
            || unsafe { (*task.header().owned_prev.get()).is_some() }
    }
}

impl<T: 'static> fmt::Debug for OwnedList<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("OwnedList").finish()
    }
}
