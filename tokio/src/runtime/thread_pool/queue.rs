//! Run-queue structures to support a work-stealing scheduler

use crate::loom::cell::{CausalCell, CausalCheck};
use crate::loom::sync::atomic::{self, AtomicU32, AtomicUsize};
use crate::loom::sync::{Arc, Mutex};
use crate::runtime::task;

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr::{self, NonNull};
use std::sync::atomic::Ordering::{Acquire, Release};

/// Producer handle. May only be used from a single thread.
pub(super) struct Local<T: 'static> {
    inner: Arc<Inner<T>>,

    /// LIFO slot. Cannot be stolen.
    next: Option<task::Notified<T>>,
}

/// Consumer handle. May be used from many threads.
pub(super) struct Steal<T: 'static>(Arc<Inner<T>>);

/// Growable, MPMC queue used to inject new tasks into the scheduler and as an
/// overflow queue when the local, fixed-size, array queue overflows.
pub(super) struct Inject<T: 'static> {
    /// Pointers to the head and tail of the queue
    pointers: Mutex<Pointers>,

    /// Number of pending tasks in the queue. This helps prevent unnecessary
    /// locking in the hot path.
    len: AtomicUsize,

    _p: PhantomData<T>,
}

pub(super) struct Inner<T: 'static> {
    /// Concurrently updated by many threads.
    head: AtomicU32,

    /// Only updated by producer thread but read by many threads.
    tail: AtomicU32,

    /// Elements
    buffer: Box<[CausalCell<MaybeUninit<task::Notified<T>>>]>,
}

struct Pointers {
    /// True if the queue is closed
    is_closed: bool,

    /// Linked-list head
    head: Option<NonNull<task::Header>>,

    /// Linked-list tail
    tail: Option<NonNull<task::Header>>,
}

unsafe impl<T> Send for Inner<T> {}
unsafe impl<T> Sync for Inner<T> {}
unsafe impl<T> Send for Inject<T> {}
unsafe impl<T> Sync for Inject<T> {}

#[cfg(not(loom))]
const LOCAL_QUEUE_CAPACITY: usize = 256;

// Shrink the size of the local queue when using loom. This shouldn't impact
// logic, but allows loom to test more edge cases in a reasonable a mount of
// time.
#[cfg(loom)]
const LOCAL_QUEUE_CAPACITY: usize = 2;

const MASK: usize = LOCAL_QUEUE_CAPACITY - 1;

/// Create a new local run-queue
pub(super) fn local<T: 'static>() -> (Steal<T>, Local<T>) {
    debug_assert!(LOCAL_QUEUE_CAPACITY >= 2 && LOCAL_QUEUE_CAPACITY.is_power_of_two());

    let mut buffer = Vec::with_capacity(LOCAL_QUEUE_CAPACITY);

    for _ in 0..LOCAL_QUEUE_CAPACITY {
        buffer.push(CausalCell::new(MaybeUninit::uninit()));
    }

    let inner = Arc::new(Inner {
        head: AtomicU32::new(0),
        tail: AtomicU32::new(0),
        buffer: buffer.into(),
    });

    let local = Local {
        inner: inner.clone(),
        next: None,
    };

    let remote = Steal(inner);

    (remote, local)
}

impl<T> Local<T> {
    /// Returns true if the queue has entries that can be stealed.
    pub(super) fn is_stealable(&self) -> bool {
        !self.inner.is_empty()
    }

    /// Returns true if the queue has an unstealable entry.
    pub(super) fn has_unstealable(&self) -> bool {
        self.next.is_some()
    }

    /// Push a task to the local queue. Returns `true` if a stealer should be
    /// notified.
    pub(super) fn push(&mut self, task: task::Notified<T>, inject: &Inject<T>) -> bool {
        let prev = self.next.take();
        let ret = prev.is_some();

        if let Some(prev) = prev {
            self.push_back(prev, inject);
        }

        self.next = Some(task);

        ret
    }

    /// Pushes a task to the back of the local queue, skipping the LIFO slot.
    pub(super) fn push_back(&mut self, mut task: task::Notified<T>, inject: &Inject<T>) {
        loop {
            let head = self.inner.head.load(Acquire);

            // safety: this is the **only** thread that updates this cell.
            let tail = unsafe { self.inner.tail.unsync_load() };

            if tail.wrapping_sub(head) < LOCAL_QUEUE_CAPACITY as u32 {
                // Map the position to a slot index.
                let idx = tail as usize & MASK;

                self.inner.buffer[idx].with_mut(|ptr| {
                    // Write the task to the slot
                    //
                    // Safety: There is only one producer and the above `if`
                    // condition ensures we don't touch a cell if there is a
                    // value, thus no consumer.
                    unsafe {
                        ptr::write((*ptr).as_mut_ptr(), task);
                    }
                });

                // Make the task available. Synchronizes with a load in
                // `steal_into2`.
                self.inner.tail.store(tail.wrapping_add(1), Release);

                return;
            }

            // The local buffer is full. Push a batch of work to the inject
            // queue.
            match self.push_overflow(task, head, tail, inject) {
                Ok(_) => return,
                // Lost the race, try again
                Err(v) => task = v,
            }

            atomic::spin_loop_hint();
        }
    }

    /// Moves a batch of tasks into the inject queue.
    ///
    /// This will temporarily make some of the tasks unavailable to stealers.
    /// Once `push_overflow` is done, a notification is sent out, so if other
    /// workers "missed" some of the tasks during a steal, they will get
    /// another opportunity.
    #[inline(never)]
    fn push_overflow(
        &mut self,
        task: task::Notified<T>,
        head: u32,
        tail: u32,
        inject: &Inject<T>,
    ) -> Result<(), task::Notified<T>> {
        const BATCH_LEN: usize = LOCAL_QUEUE_CAPACITY / 2 + 1;

        let n = tail.wrapping_sub(head) / 2;
        debug_assert_eq!(n as usize, LOCAL_QUEUE_CAPACITY / 2, "queue is not full");

        // Claim a bunch of tasks
        //
        // We are claiming the tasks **before** reading them out of the buffer.
        // This is safe because only the **current** thread is able to push new
        // tasks.
        //
        // There isn't really any need for memory ordering... Relaxed would
        // work. This is because all tasks are pushed into the queue from the
        // current thread (or memory has been acquired if the local queue handle
        // moved).
        let actual = self.inner.head.compare_and_swap(head, head + n, Release);
        if actual != head {
            // We failed to claim the tasks, losing the race. Return out of
            // this function and try the full `push` routine again. The queue
            // may not be full anymore.
            return Err(task);
        }

        // link the tasks
        for i in 0..n {
            let j = i + 1;

            let i_idx = (i + head) as usize & MASK;
            let j_idx = (j + head) as usize & MASK;

            // Get the next pointer
            let next = if j == n {
                // The last task in the local queue being moved
                task.header().into()
            } else {
                // safety: The above CAS prevents a stealer from accessing these
                // tasks and we are the only producer.
                self.inner.buffer[j_idx].with(|ptr| unsafe {
                    let value = (*ptr).as_ptr();
                    (*value).header().into()
                })
            };

            // safety: the above CAS prevents a stealer from accessing these
            // tasks and we are the only producer.
            self.inner.buffer[i_idx].with_mut(|ptr| unsafe {
                let ptr = (*ptr).as_ptr();
                *(*ptr).header().queue_next.get() = Some(next);
            });
        }

        // safety: the above CAS prevents a stealer from accessing these tasks
        // and we are the only producer.
        let head = self.inner.buffer[head as usize & MASK]
            .with(|ptr| unsafe { ptr::read((*ptr).as_ptr()) });

        // Push the tasks onto the inject queue
        inject.push_batch(head, task, BATCH_LEN);

        Ok(())
    }

    /// Pops a task from the local queue.
    pub(super) fn pop(&mut self) -> Option<task::Notified<T>> {
        // If a task is available in the FIFO slot, return that.
        if let Some(task) = self.next.take() {
            return Some(task);
        }

        loop {
            let head = self.inner.head.load(Acquire);

            // safety: this is the **only** thread that updates this cell.
            let tail = unsafe { self.inner.tail.unsync_load() };

            if head == tail {
                // queue is empty
                return None;
            }

            // Map the head position to a slot index.
            let idx = head as usize & MASK;

            let task = self.inner.buffer[idx].with(|ptr| {
                // Tentatively read the task at the head position. Note that we
                // have not yet claimed the task.
                //
                // safety: reading this as uninitialized memory.
                unsafe { ptr::read(ptr) }
            });

            // Attempt to claim the task read above.
            let actual = self
                .inner
                .head
                .compare_and_swap(head, head.wrapping_add(1), Release);

            if actual == head {
                // safety: we claimed the task and the data we read is
                // initialized memory.
                return Some(unsafe { task.assume_init() });
            }

            atomic::spin_loop_hint();
        }
    }
}

impl<T> Steal<T> {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Steals half the tasks from self and place them into `dst`.
    pub(super) fn steal_into(&self, dst: &mut Local<T>) -> Option<task::Notified<T>> {
        // Safety: the caller is the only thread that mutates `dst.tail` and
        // holds a mutable reference.
        let dst_tail = unsafe { dst.inner.tail.unsync_load() };

        // Steal the tasks into `dst`'s buffer. This does not yet expose the
        // tasks in `dst`.
        let mut n = self.steal_into2(dst, dst_tail);

        if n == 0 {
            // No tasks were stolen
            return None;
        }

        // We are returning a task here
        n -= 1;

        let ret_pos = dst_tail.wrapping_add(n);
        let ret_idx = ret_pos as usize & MASK;

        // safety: the value was written as part of `steal_into2` and not
        // exposed to stealers, so no other thread can access it.
        let ret = dst.inner.buffer[ret_idx].with(|ptr| unsafe { ptr::read((*ptr).as_ptr()) });

        if n == 0 {
            // The `dst` queue is empty, but a single task was stolen
            return Some(ret);
        }

        // Synchronize with stealers
        let dst_head = dst.inner.head.load(Acquire);

        assert!(dst_tail.wrapping_sub(dst_head) + n <= LOCAL_QUEUE_CAPACITY as u32);

        // Make the stolen items available to consumers
        dst.inner.tail.store(dst_tail.wrapping_add(n), Release);

        Some(ret)
    }

    fn steal_into2(&self, dst: &mut Local<T>, dst_tail: u32) -> u32 {
        loop {
            let src_head = self.0.head.load(Acquire);
            let src_tail = self.0.tail.load(Acquire);

            // Number of available tasks to steal
            let n = src_tail.wrapping_sub(src_head);
            let n = n - n / 2;

            if n == 0 {
                return 0;
            }

            if n > LOCAL_QUEUE_CAPACITY as u32 / 2 {
                atomic::spin_loop_hint();
                // inconsistent, try again
                continue;
            }

            // Track CausalCell causality checks. The check is deferred until
            // the compare_and_swap claims ownership of the tasks.
            let mut check = CausalCheck::default();

            for i in 0..n {
                // Compute the positions
                let src_pos = src_head.wrapping_add(i);
                let dst_pos = dst_tail.wrapping_add(i);

                // Map to slots
                let src_idx = src_pos as usize & MASK;
                let dst_idx = dst_pos as usize & MASK;

                // Read the task
                //
                // safety: this is being read as MaybeUninit -- potentially
                // uninitialized memory (in the case a producer wraps). We don't
                // assume it is initialized, but will just write the
                // `MaybeUninit` in our slot below.
                let (task, ch) = self.0.buffer[src_idx]
                    .with_deferred(|ptr| unsafe { ptr::read((*ptr).as_ptr()) });

                check.join(ch);

                // Write the task to the new slot
                //
                // safety: `dst` queue is empty and we are the only producer to
                // this queue.
                dst.inner.buffer[dst_idx]
                    .with_mut(|ptr| unsafe { ptr::write((*ptr).as_mut_ptr(), task) });
            }

            // Claim all of those tasks!
            let actual = self
                .0
                .head
                .compare_and_swap(src_head, src_head.wrapping_add(n), Release);

            if actual == src_head {
                check.check();
                return n;
            }

            atomic::spin_loop_hint();
        }
    }
}

impl<T> Drop for Local<T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.pop().is_none(), "queue not empty");
        }
    }
}

impl<T> Inner<T> {
    fn is_empty(&self) -> bool {
        let head = self.head.load(Acquire);
        let tail = self.tail.load(Acquire);

        head == tail
    }
}

impl<T: 'static> Inject<T> {
    pub(super) fn new() -> Inject<T> {
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

    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Close the injection queue, returns `true` if the queue is open when the
    /// transition is made.
    pub(super) fn close(&self) -> bool {
        let mut p = self.pointers.lock().unwrap();

        if p.is_closed {
            return false;
        }

        p.is_closed = true;
        true
    }

    pub(super) fn is_closed(&self) -> bool {
        self.pointers.lock().unwrap().is_closed
    }

    fn len(&self) -> usize {
        self.len.load(Acquire)
    }

    /// Pushes a value into the queue.
    pub(super) fn push(&self, task: task::Notified<T>) {
        // Acquire queue lock
        let mut p = self.pointers.lock().unwrap();

        if p.is_closed {
            // Drop the mutex to avoid a potential deadlock when
            // re-entering.
            drop(p);
            drop(task);
            return;
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
    }

    pub(super) fn push_batch(
        &self,
        batch_head: task::Notified<T>,
        batch_tail: task::Notified<T>,
        num: usize,
    ) {
        let batch_head = batch_head.into_raw();
        let batch_tail = batch_tail.into_raw();

        debug_assert!(get_next(batch_tail).is_none());

        let mut p = self.pointers.lock().unwrap();

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

    pub(super) fn pop(&self) -> Option<task::Notified<T>> {
        // Fast path, if len == 0, then there are no values
        if self.is_empty() {
            return None;
        }

        let mut p = self.pointers.lock().unwrap();

        // It is possible to hit null here if another thread poped the last
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
    unsafe { *header.as_ref().queue_next.get() }
}

fn set_next(header: NonNull<task::Header>, val: Option<NonNull<task::Header>>) {
    unsafe {
        *header.as_ref().queue_next.get() = val;
    }
}
