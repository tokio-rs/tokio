use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Mutex;
use crate::task::{Header, Task};

use std::marker::PhantomData;
use std::ptr::{self, NonNull};
use std::sync::atomic::Ordering::{Acquire, Release};
use std::usize;

pub(super) struct Queue<T: 'static> {
    /// Pointers to the head and tail of the queue
    pointers: Mutex<Pointers>,

    /// Number of pending tasks in the queue. This helps prevent unnecessary
    /// locking in the hot path.
    ///
    /// The LSB is a flag tracking whether or not the queue is open or not.
    len: AtomicUsize,

    _p: PhantomData<T>,
}

struct Pointers {
    head: *const Header,
    tail: *const Header,
}

const CLOSED: usize = 1;
const MAX_LEN: usize = usize::MAX >> 1;

impl<T: 'static> Queue<T> {
    pub(super) fn new() -> Queue<T> {
        Queue {
            pointers: Mutex::new(Pointers {
                head: ptr::null(),
                tail: ptr::null(),
            }),
            len: AtomicUsize::new(0),
            _p: PhantomData,
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(super) fn is_closed(&self) -> bool {
        self.len.load(Acquire) & CLOSED == CLOSED
    }

    /// Close the worker queue
    pub(super) fn close(&self) -> bool {
        // Acquire the lock
        let p = self.pointers.lock().unwrap();

        let len = unsafe {
            // Set the queue as closed. Because all mutations are synchronized by
            // the mutex, a read followed by a write is acceptable.
            self.len.unsync_load()
        };

        let ret = len & CLOSED == 0;

        self.len.store(len | CLOSED, Release);

        drop(p);

        ret
    }

    fn len(&self) -> usize {
        self.len.load(Acquire) >> 1
    }

    pub(super) fn wait_for_unlocked(&self) {
        // Acquire and release the lock immediately. This synchronizes the
        // caller **after** all external waiters are done w/ the scheduler
        // struct.
        drop(self.pointers.lock().unwrap());
    }

    /// Pushes a value into the queue and call the closure **while still holding
    /// the push lock**
    pub(super) fn push<F>(&self, task: Task<T>, f: F)
    where
        F: FnOnce(Result<(), Task<T>>),
    {
        unsafe {
            // Acquire queue lock
            let mut p = self.pointers.lock().unwrap();

            // Check if the queue is closed. This must happen in the lock.
            let len = self.len.unsync_load();
            if len & CLOSED == CLOSED {
                drop(p);
                f(Err(task));
                return;
            }

            let task = task.into_raw();

            // The next pointer should already be null
            debug_assert!(get_next(task).is_null());

            if let Some(tail) = NonNull::new(p.tail as *mut _) {
                set_next(tail, task.as_ptr());
            } else {
                p.head = task.as_ptr();
            }

            p.tail = task.as_ptr();

            // Increment the count.
            //
            // All updates to the len atomic are guarded by the mutex. As such,
            // a non-atomic load followed by a store is safe.
            //
            // We increment by 2 to avoid touching the shutdown flag
            if (len >> 1) == MAX_LEN {
                eprintln!("[ERROR] overflowed task counter. This is a bug and should be reported.");
                std::process::abort();
            }

            self.len.store(len + 2, Release);

            f(Ok(()));

            drop(p);
        }
    }

    pub(super) fn push_batch(&self, batch_head: Task<T>, batch_tail: Task<T>, num: usize) {
        unsafe {
            let batch_head = batch_head.into_raw().as_ptr();
            let batch_tail = batch_tail.into_raw();

            debug_assert!(get_next(batch_tail).is_null());

            let mut p = self.pointers.lock().unwrap();

            if let Some(tail) = NonNull::new(p.tail as *mut _) {
                set_next(tail, batch_head);
            } else {
                p.head = batch_head;
            }

            p.tail = batch_tail.as_ptr();

            // Increment the count.
            //
            // All updates to the len atomic are guarded by the mutex. As such,
            // a non-atomic load followed by a store is safe.
            //
            // Left shift by 1 to avoid touching the shutdown flag.
            let len = self.len.unsync_load();

            if (len >> 1) >= (MAX_LEN - num) {
                std::process::abort();
            }

            self.len.store(len + (num << 1), Release);

            drop(p);
        }
    }

    pub(super) fn pop(&self) -> Option<Task<T>> {
        // Fast path, if len == 0, then there are no values
        if self.is_empty() {
            return None;
        }

        unsafe {
            let mut p = self.pointers.lock().unwrap();

            // It is possible to hit null here if another thread poped the last
            // task between us checking `len` and acquiring the lock.
            let task = NonNull::new(p.head as *mut _)?;

            p.head = get_next(task);

            if p.head.is_null() {
                p.tail = ptr::null();
            }

            set_next(task, ptr::null());

            // Decrement the count.
            //
            // All updates to the len atomic are guarded by the mutex. As such,
            // a non-atomic load followed by a store is safe.
            //
            // Decrement by 2 to avoid touching the shutdown flag
            self.len.store(self.len.unsync_load() - 2, Release);

            drop(p);

            Some(Task::from_raw(task))
        }
    }
}

unsafe fn get_next(meta: NonNull<Header>) -> *const Header {
    *meta.as_ref().queue_next.get()
}

unsafe fn set_next(meta: NonNull<Header>, val: *const Header) {
    *meta.as_ref().queue_next.get() = val;
}
