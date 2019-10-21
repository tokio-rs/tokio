use crate::loom::sync::atomic::AtomicPtr;
use crate::task::{Header, Task};

use std::ptr::{self, NonNull};
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

/// Concurrent stack of tasks, used to pass ownership of a task from one worker
/// to another.
pub(crate) struct TransferStack<T: 'static> {
    head: AtomicPtr<Header<T>>,
}

impl<T: 'static> TransferStack<T> {
    pub(crate) fn new() -> TransferStack<T> {
        TransferStack {
            head: AtomicPtr::new(ptr::null_mut()),
        }
    }

    pub(crate) fn push(&self, task: Task<T>) {
        unsafe {
            let task = task.into_raw();

            let next = (*task.as_ref().queue_next.get()) as usize;

            // At this point, the queue_next field may also be used to track
            // whether or not the task must drop the join waker.
            debug_assert_eq!(0, next & 1);

            // We don't care about any memory associated w/ setting the `head`
            // field, just the current value.
            let mut curr = self.head.load(Relaxed);

            loop {
                *task.as_ref().queue_next.get() = (next | curr as usize) as *const _;

                let res =
                    self.head
                        .compare_exchange(curr, task.as_ptr() as *mut _, Release, Relaxed);

                match res {
                    Ok(_) => return,
                    Err(actual) => {
                        curr = actual;
                    }
                }
            }
        }
    }

    pub(crate) fn drain(&self) -> impl Iterator<Item = Task<T>> {
        struct Iter<T: 'static>(*mut Header<T>);

        impl<T: 'static> Iterator for Iter<T> {
            type Item = Task<T>;

            fn next(&mut self) -> Option<Task<T>> {
                let task = NonNull::new(self.0)?;

                unsafe {
                    let next = *task.as_ref().queue_next.get() as usize;

                    // remove the data bit
                    self.0 = (next & !1) as *mut _;

                    Some(Task::from_raw(task))
                }
            }
        }

        impl<T: 'static> Drop for Iter<T> {
            fn drop(&mut self) {
                use std::process;

                if !self.0.is_null() {
                    // we have bugs
                    process::abort();
                }
            }
        }

        let ptr = self.head.swap(ptr::null_mut(), Acquire);
        Iter(ptr)
    }
}
