use task::Task;

use std::cell::UnsafeCell;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering::{Acquire, Release, AcqRel, Relaxed};

#[derive(Debug)]
pub(crate) struct Queue {
    /// Queue head.
    ///
    /// This is a strong reference to `Task` (i.e, `Arc<Task>`)
    head: AtomicPtr<Task>,

    /// Tail pointer. This is `Arc<Task>`.
    tail: UnsafeCell<*mut Task>,

    /// Stub pointer, used as part of the intrusive mpsc channel algorithm
    /// described by 1024cores.
    stub: Box<Task>,
}

#[derive(Debug)]
pub(crate) enum Poll {
    Empty,
    Inconsistent,
    Data(Arc<Task>),
}

// ===== impl Queue =====

impl Queue {
    /// Create a new, empty, `Queue`.
    pub fn new() -> Queue {
        let stub = Box::new(Task::stub());
        let ptr = &*stub as *const _ as *mut _;

        Queue {
            head: AtomicPtr::new(ptr),
            tail: UnsafeCell::new(ptr),
            stub: stub,
        }
    }

    /// Push a task onto the queue.
    ///
    /// This function is `Sync`.
    pub fn push(&self, task: Arc<Task>) {
        unsafe {
            self.push2(Arc::into_raw(task));
        }
    }

    unsafe fn push2(&self, task: *const Task) {
        let task = task as *mut Task;

        // Set the next pointer. This does not require an atomic operation as
        // this node is not accessible. The write will be flushed with the next
        // operation
        (*task).next.store(ptr::null_mut(), Relaxed);

        // Update the head to point to the new node. We need to see the previous
        // node in order to update the next pointer as well as release `task`
        // to any other threads calling `push`.
        let prev = self.head.swap(task, AcqRel);

        // Release `task` to the consume end.
        (*prev).next.store(task, Release);
    }

    /// Poll a task from the queue.
    ///
    /// This function is **not** `Sync` and requires coordination by the caller.
    pub unsafe fn poll(&self) -> Poll {
        let mut tail = *self.tail.get();
        let mut next = (*tail).next.load(Acquire);

        let stub = &*self.stub as *const _ as *mut _;

        if tail == stub {
            if next.is_null() {
                return Poll::Empty;
            }

            *self.tail.get() = next;
            tail = next;
            next = (*next).next.load(Acquire);
        }

        if !next.is_null() {
            *self.tail.get() = next;

            // No ref_count inc is necessary here as this poll is paired
            // with a `push` which "forgets" the handle.
            return Poll::Data(Arc::from_raw(tail));
        }

        if self.head.load(Acquire) != tail {
            return Poll::Inconsistent;
        }

        self.push2(stub);

        next = (*tail).next.load(Acquire);

        if !next.is_null() {
            *self.tail.get() = next;

            return Poll::Data(Arc::from_raw(tail));
        }

        Poll::Inconsistent
    }
}
