use crate::loom::sync::atomic::AtomicPtr;
use crate::runtime::task::{Header, Task};

use std::marker::PhantomData;
use std::ptr::{self, NonNull};
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

/// Concurrent stack of tasks, used to pass ownership of a task from one worker
/// to another.
pub(crate) struct TransferStack<T: 'static> {
    head: AtomicPtr<Header>,
    _p: PhantomData<T>,
}

impl<T: 'static> TransferStack<T> {
    pub(crate) fn new() -> TransferStack<T> {
        TransferStack {
            head: AtomicPtr::new(ptr::null_mut()),
            _p: PhantomData,
        }
    }

    pub(crate) fn push(&self, task: Task<T>) {
        let task = task.into_raw();

        // We don't care about any memory associated w/ setting the `head`
        // field, just the current value.
        //
        // The compare-exchange creates a release sequence.
        let mut curr = self.head.load(Relaxed);

        loop {
            unsafe {
                task.as_ref()
                    .stack_next
                    .with_mut(|ptr| *ptr = NonNull::new(curr))
            };

            let res = self
                .head
                .compare_exchange(curr, task.as_ptr() as *mut _, Release, Relaxed);

            match res {
                Ok(_) => return,
                Err(actual) => {
                    curr = actual;
                }
            }
        }
    }

    pub(crate) fn drain(&self) -> impl Iterator<Item = Task<T>> {
        struct Iter<T: 'static>(Option<NonNull<Header>>, PhantomData<T>);

        impl<T: 'static> Iterator for Iter<T> {
            type Item = Task<T>;

            fn next(&mut self) -> Option<Task<T>> {
                let task = self.0?;

                // Move the cursor forward
                self.0 = unsafe { task.as_ref().stack_next.with(|ptr| *ptr) };

                // Return the task
                unsafe { Some(Task::from_raw(task)) }
            }
        }

        impl<T: 'static> Drop for Iter<T> {
            fn drop(&mut self) {
                use std::process;

                if self.0.is_some() {
                    // we have bugs
                    process::abort();
                }
            }
        }

        let ptr = self.head.swap(ptr::null_mut(), Acquire);
        Iter(NonNull::new(ptr), PhantomData)
    }
}
