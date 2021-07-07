//! Inject queue used to send wakeups to a work-stealing scheduler

use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Mutex;
use crate::runtime::task;

use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::{Acquire, Release};

/// Growable, MPMC queue used to inject new tasks into the scheduler and as an
/// overflow queue when the local, fixed-size, array queue overflows.
pub(crate) struct Inject<T: 'static> {
    /// Pointers to the head and tail of the queue
    pointers: Mutex<Pointers>,

    /// Number of pending tasks in the queue. This helps prevent unnecessary
    /// locking in the hot path.
    len: AtomicUsize,

    _p: PhantomData<T>,
}

struct Pointers {
    /// True if the queue is closed
    is_closed: bool,

    /// Linked-list head
    head: Option<NonNull<task::Header>>,

    /// Linked-list tail
    tail: Option<NonNull<task::Header>>,
}

unsafe impl<T> Send for Inject<T> {}
unsafe impl<T> Sync for Inject<T> {}

impl<T: 'static> Inject<T> {
    pub(crate) fn new() -> Inject<T> {
        Inject {
            pointers: Mutex::new(Pointers {
                is_closed: false,
                head: None,
                tail: None,
            }),
            len: AtomicUsize::new(0),
            _p: PhantomData,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Close the injection queue, returns `true` if the queue is open when the
    /// transition is made.
    pub(crate) fn close(&self) -> bool {
        let mut p = self.pointers.lock();

        if p.is_closed {
            return false;
        }

        p.is_closed = true;
        true
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.pointers.lock().is_closed
    }

    pub(crate) fn len(&self) -> usize {
        self.len.load(Acquire)
    }

    /// Pushes a value into the queue.
    ///
    /// Returns `Err(task)` if pushing fails due to the queue being shutdown.
    /// The caller is expected to call `shutdown()` on the task **if and only
    /// if** it is a newly spawned task.
    pub(crate) fn push(&self, task: task::Notified<T>) -> Result<(), task::Notified<T>>
    where
        T: crate::runtime::task::Schedule,
    {
        // Acquire queue lock
        let mut p = self.pointers.lock();

        if p.is_closed {
            return Err(task);
        }

        // safety: only mutated with the lock held
        let len = unsafe { self.len.unsync_load() };
        let task = task.into_raw();

        // The next pointer should already be null
        debug_assert!(get_next(task).is_none());

        if let Some(tail) = p.tail {
            set_next(tail, Some(task));
        } else {
            p.head = Some(task);
        }

        p.tail = Some(task);

        self.len.store(len + 1, Release);
        Ok(())
    }

    /// Pushes several values into the queue.
    ///
    /// SAFETY: The caller should ensure that we have exclusive access to the
    /// `queue_next` field in the provided tasks.
    #[inline]
    pub(crate) fn push_batch<I>(&self, mut iter: I)
    where
        I: Iterator<Item = task::Notified<T>>,
    {
        let first = match iter.next() {
            Some(first) => first.into_raw(),
            None => return,
        };

        // Link up all the tasks.
        let mut prev = first;
        let mut counter = 1;

        // We are going to be called with an `std::iter::Chain`, and that
        // iterator overrides `for_each` to something that is easier for the
        // compiler to optimize than a loop.
        iter.map(|next| next.into_raw()).for_each(|next| {
            // safety: The caller guarantees exclusive access to this field.
            set_next(prev, Some(next));
            prev = next;
            counter += 1;
        });

        // Now that the tasks are linked together, insert them into the
        // linked list.
        self.push_batch_inner(first, prev, counter);
    }

    /// Insert several tasks that have been linked together into the queue.
    ///
    /// The provided head and tail may be be the same task. In this case, a
    /// single task is inserted.
    #[inline]
    fn push_batch_inner(
        &self,
        batch_head: NonNull<task::Header>,
        batch_tail: NonNull<task::Header>,
        num: usize,
    ) {
        debug_assert!(get_next(batch_tail).is_none());

        let mut p = self.pointers.lock();

        if let Some(tail) = p.tail {
            set_next(tail, Some(batch_head));
        } else {
            p.head = Some(batch_head);
        }

        p.tail = Some(batch_tail);

        // Increment the count.
        //
        // safety: All updates to the len atomic are guarded by the mutex. As
        // such, a non-atomic load followed by a store is safe.
        let len = unsafe { self.len.unsync_load() };

        self.len.store(len + num, Release);
    }

    pub(crate) fn pop(&self) -> Option<task::Notified<T>> {
        // Fast path, if len == 0, then there are no values
        if self.is_empty() {
            return None;
        }

        let mut p = self.pointers.lock();

        // It is possible to hit null here if another thread popped the last
        // task between us checking `len` and acquiring the lock.
        let task = p.head?;

        p.head = get_next(task);

        if p.head.is_none() {
            p.tail = None;
        }

        set_next(task, None);

        // Decrement the count.
        //
        // safety: All updates to the len atomic are guarded by the mutex. As
        // such, a non-atomic load followed by a store is safe.
        self.len
            .store(unsafe { self.len.unsync_load() } - 1, Release);

        // safety: a `Notified` is pushed into the queue and now it is popped!
        Some(unsafe { task::Notified::from_raw(task) })
    }
}

impl<T: 'static> Drop for Inject<T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.pop().is_none(), "queue not empty");
        }
    }
}

fn get_next(header: NonNull<task::Header>) -> Option<NonNull<task::Header>> {
    unsafe { header.as_ref().queue_next.with(|ptr| *ptr) }
}

fn set_next(header: NonNull<task::Header>, val: Option<NonNull<task::Header>>) {
    unsafe {
        header.as_ref().set_next(val);
    }
}
