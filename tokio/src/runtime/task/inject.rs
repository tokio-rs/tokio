//! Inject queue used to send wakeups to a work-stealing scheduler

use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::{Mutex, MutexGuard};
use crate::runtime::task;

use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::{Acquire, Release};

/// Growable, MPMC queue used to inject new tasks into the scheduler and as an
/// overflow queue when the local, fixed-size, array queue overflows.
pub(crate) struct Inject<T: 'static> {
    /// Pointers to the head and tail of the queue.
    pointers: Mutex<Pointers>,

    /// Number of pending tasks in the queue. This helps prevent unnecessary
    /// locking in the hot path.
    len: AtomicUsize,

    _p: PhantomData<T>,
}

struct Pointers {
    /// True if the queue is closed.
    is_closed: bool,

    /// Linked-list head.
    head: Option<NonNull<task::Header>>,

    /// Linked-list tail.
    tail: Option<NonNull<task::Header>>,
}

pub(crate) struct Pop<'a, T: 'static> {
    len: usize,
    pointers: Option<MutexGuard<'a, Pointers>>,
    _p: PhantomData<T>,
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

    // Kind of annoying to have to include the cfg here
    #[cfg(any(tokio_taskdump, all(feature = "rt-multi-thread", not(tokio_wasi))))]
    pub(crate) fn is_closed(&self) -> bool {
        self.pointers.lock().is_closed
    }

    /// Closes the injection queue, returns `true` if the queue is open when the
    /// transition is made.
    pub(crate) fn close(&self) -> bool {
        let mut p = self.pointers.lock();

        if p.is_closed {
            return false;
        }

        p.is_closed = true;
        true
    }

    pub(crate) fn len(&self) -> usize {
        self.len.load(Acquire)
    }

    /// Pushes a value into the queue.
    ///
    /// This does nothing if the queue is closed.
    pub(crate) fn push(&self, task: task::Notified<T>) {
        // Acquire queue lock
        let mut p = self.pointers.lock();

        if p.is_closed {
            return;
        }

        // safety: only mutated with the lock held
        let len = unsafe { self.len.unsync_load() };
        let task = task.into_raw();

        // The next pointer should already be null
        debug_assert!(get_next(task).is_none());

        if let Some(tail) = p.tail {
            // safety: Holding the Notified for a task guarantees exclusive
            // access to the `queue_next` field.
            set_next(tail, Some(task));
        } else {
            p.head = Some(task);
        }

        p.tail = Some(task);

        self.len.store(len + 1, Release);
    }

    pub(crate) fn pop(&self) -> Option<task::Notified<T>> {
        self.pop_n(1).next()
    }

    pub(crate) fn pop_n(&self, n: usize) -> Pop<'_, T> {
        use std::cmp;

        // Fast path, if len == 0, then there are no values
        if self.is_empty() {
            return Pop {
                len: 0,
                pointers: None,
                _p: PhantomData,
            };
        }

        // Lock the queue
        let p = self.pointers.lock();

        // safety: All updates to the len atomic are guarded by the mutex. As
        // such, a non-atomic load followed by a store is safe.
        let len = unsafe { self.len.unsync_load() };

        let n = cmp::min(n, len);

        // Decrement the count.
        self.len.store(len - n, Release);

        Pop {
            len: n,
            pointers: Some(p),
            _p: PhantomData,
        }
    }
}

cfg_rt_multi_thread! {
    impl<T: 'static> Inject<T> {
        /// Pushes several values into the queue.
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
            iter.for_each(|next| {
                let next = next.into_raw();

                // safety: Holding the Notified for a task guarantees exclusive
                // access to the `queue_next` field.
                set_next(prev, Some(next));
                prev = next;
                counter += 1;
            });

            // Now that the tasks are linked together, insert them into the
            // linked list.
            self.push_batch_inner(first, prev, counter);
        }

        /// Inserts several tasks that have been linked together into the queue.
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
    }
}

impl<T: 'static> Drop for Inject<T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.pop().is_none(), "queue not empty");
        }
    }
}

impl<'a, T: 'static> Iterator for Pop<'a, T> {
    type Item = task::Notified<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }

        // `pointers` is always `Some` when `len() > 0`
        let pointers = self.pointers.as_mut().unwrap();
        let ret = pointers.pop();

        debug_assert!(ret.is_some());

        self.len -= 1;

        if self.len == 0 {
            self.pointers = None;
        }

        ret
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'a, T: 'static> ExactSizeIterator for Pop<'a, T> {
    fn len(&self) -> usize {
        self.len
    }
}

impl<'a, T: 'static> Drop for Pop<'a, T> {
    fn drop(&mut self) {
        for _ in self.by_ref() {}
    }
}

impl Pointers {
    fn pop<T: 'static>(&mut self) -> Option<task::Notified<T>> {
        let task = self.head?;

        self.head = get_next(task);

        if self.head.is_none() {
            self.tail = None;
        }

        set_next(task, None);

        // safety: a `Notified` is pushed into the queue and now it is popped!
        Some(unsafe { task::Notified::from_raw(task) })
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
