//! Run-queue structures to support a work-stealing scheduler

use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::{AtomicU16, AtomicU32, AtomicUsize};
use crate::loom::sync::{Arc, Mutex};
use crate::runtime::task;

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr::{self, NonNull};
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};

/// Producer handle. May only be used from a single thread.
pub(super) struct Local<T: 'static> {
    inner: Arc<Inner<T>>,
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
    ///
    /// Contains two `u16` values. The LSB byte is the "real" head of the queue.
    /// The `u16` in the MSB is set by a stealer in process of stealing values.
    /// It represents the first value being stolen in the batch. `u16` is used
    /// in order to distinguish between `head == tail` and `head == tail -
    /// capacity`.
    ///
    /// When both `u16` values are the same, there is no active stealer.
    ///
    /// Tracking an in-progress stealer prevents a wrapping scenario.
    head: AtomicU32,

    /// Only updated by producer thread but read by many threads.
    tail: AtomicU16,

    /// Elements
    buffer: Box<[UnsafeCell<MaybeUninit<task::Notified<T>>>]>,
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
const LOCAL_QUEUE_CAPACITY: usize = 4;

const MASK: usize = LOCAL_QUEUE_CAPACITY - 1;

/// Create a new local run-queue
pub(super) fn local<T: 'static>() -> (Steal<T>, Local<T>) {
    let mut buffer = Vec::with_capacity(LOCAL_QUEUE_CAPACITY);

    for _ in 0..LOCAL_QUEUE_CAPACITY {
        buffer.push(UnsafeCell::new(MaybeUninit::uninit()));
    }

    let inner = Arc::new(Inner {
        head: AtomicU32::new(0),
        tail: AtomicU16::new(0),
        buffer: buffer.into(),
    });

    let local = Local {
        inner: inner.clone(),
    };

    let remote = Steal(inner);

    (remote, local)
}

impl<T> Local<T> {
    /// Returns true if the queue has entries that can be stealed.
    pub(super) fn is_stealable(&self) -> bool {
        !self.inner.is_empty()
    }

    /// Pushes a task to the back of the local queue, skipping the LIFO slot.
    pub(super) fn push_back(&mut self, mut task: task::Notified<T>, inject: &Inject<T>) {
        let tail = loop {
            let head = self.inner.head.load(Acquire);
            let (steal, real) = unpack(head);

            // safety: this is the **only** thread that updates this cell.
            let tail = unsafe { self.inner.tail.unsync_load() };

            if tail.wrapping_sub(steal) < LOCAL_QUEUE_CAPACITY as u16 {
                // There is capacity for the task
                break tail;
            } else if steal != real {
                // Concurrently stealing, this will free up capacity, so
                // only push the new task onto the inject queue
                inject.push(task);
                return;
            } else {
                // Push the current task and half of the queue into the
                // inject queue.
                match self.push_overflow(task, real, tail, inject) {
                    Ok(_) => return,
                    // Lost the race, try again
                    Err(v) => {
                        task = v;
                    }
                }
            }
        };

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
        head: u16,
        tail: u16,
        inject: &Inject<T>,
    ) -> Result<(), task::Notified<T>> {
        const BATCH_LEN: usize = LOCAL_QUEUE_CAPACITY / 2 + 1;

        let n = (LOCAL_QUEUE_CAPACITY / 2) as u16;
        assert_eq!(
            tail.wrapping_sub(head) as usize,
            LOCAL_QUEUE_CAPACITY,
            "queue is not full; tail = {}; head = {}",
            tail,
            head
        );

        let prev = pack(head, head);

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
        let actual = self.inner.head.compare_and_swap(
            prev,
            pack(head.wrapping_add(n), head.wrapping_add(n)),
            Release,
        );

        if actual != prev {
            // We failed to claim the tasks, losing the race. Return out of
            // this function and try the full `push` routine again. The queue
            // may not be full anymore.
            return Err(task);
        }

        // link the tasks
        for i in 0..n {
            let j = i + 1;

            let i_idx = i.wrapping_add(head) as usize & MASK;
            let j_idx = j.wrapping_add(head) as usize & MASK;

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
                (*ptr).header().queue_next.with_mut(|ptr| *ptr = Some(next));
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
        let mut head = self.inner.head.load(Acquire);

        let idx = loop {
            let (steal, real) = unpack(head);

            // safety: this is the **only** thread that updates this cell.
            let tail = unsafe { self.inner.tail.unsync_load() };

            if real == tail {
                // queue is empty
                return None;
            }

            let next_real = real.wrapping_add(1);

            // If `steal == real` there are no concurrent stealers. Both `steal`
            // and `real` are updated.
            let next = if steal == real {
                pack(next_real, next_real)
            } else {
                assert_ne!(steal, next_real);
                pack(steal, next_real)
            };

            // Attempt to claim a task.
            let res = self
                .inner
                .head
                .compare_exchange(head, next, AcqRel, Acquire);

            match res {
                Ok(_) => break real as usize & MASK,
                Err(actual) => head = actual,
            }
        };

        Some(self.inner.buffer[idx].with(|ptr| unsafe { ptr::read(ptr).assume_init() }))
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

        // To the caller, `dst` may **look** empty but still have values
        // contained in the buffer. If another thread is concurrently stealing
        // from `dst` there may not be enough capacity to steal.
        let (steal, _) = unpack(dst.inner.head.load(Acquire));

        if dst_tail.wrapping_sub(steal) > LOCAL_QUEUE_CAPACITY as u16 / 2 {
            // we *could* try to steal less here, but for simplicity, we're just
            // going to abort.
            return None;
        }

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

        // Make the stolen items available to consumers
        dst.inner.tail.store(dst_tail.wrapping_add(n), Release);

        Some(ret)
    }

    // Steal tasks from `self`, placing them into `dst`. Returns the number of
    // tasks that were stolen.
    fn steal_into2(&self, dst: &mut Local<T>, dst_tail: u16) -> u16 {
        let mut prev_packed = self.0.head.load(Acquire);
        let mut next_packed;

        let n = loop {
            let (src_head_steal, src_head_real) = unpack(prev_packed);
            let src_tail = self.0.tail.load(Acquire);

            // If these two do not match, another thread is concurrently
            // stealing from the queue.
            if src_head_steal != src_head_real {
                return 0;
            }

            // Number of available tasks to steal
            let n = src_tail.wrapping_sub(src_head_real);
            let n = n - n / 2;

            if n == 0 {
                // No tasks available to steal
                return 0;
            }

            // Update the real head index to acquire the tasks.
            let steal_to = src_head_real.wrapping_add(n);
            assert_ne!(src_head_steal, steal_to);
            next_packed = pack(src_head_steal, steal_to);

            // Claim all those tasks. This is done by incrementing the "real"
            // head but not the steal. By doing this, no other thread is able to
            // steal from this queue until the current thread completes.
            let res = self
                .0
                .head
                .compare_exchange(prev_packed, next_packed, AcqRel, Acquire);

            match res {
                Ok(_) => break n,
                Err(actual) => prev_packed = actual,
            }
        };

        assert!(n <= LOCAL_QUEUE_CAPACITY as u16 / 2, "actual = {}", n);

        let (first, _) = unpack(next_packed);

        // Take all the tasks
        for i in 0..n {
            // Compute the positions
            let src_pos = first.wrapping_add(i);
            let dst_pos = dst_tail.wrapping_add(i);

            // Map to slots
            let src_idx = src_pos as usize & MASK;
            let dst_idx = dst_pos as usize & MASK;

            // Read the task
            //
            // safety: We acquired the task with the atomic exchange above.
            let task = self.0.buffer[src_idx].with(|ptr| unsafe { ptr::read((*ptr).as_ptr()) });

            // Write the task to the new slot
            //
            // safety: `dst` queue is empty and we are the only producer to
            // this queue.
            dst.inner.buffer[dst_idx]
                .with_mut(|ptr| unsafe { ptr::write((*ptr).as_mut_ptr(), task) });
        }

        let mut prev_packed = next_packed;

        // Update `src_head_steal` to match `src_head_real` signalling that the
        // stealing routine is complete.
        loop {
            let head = unpack(prev_packed).1;
            next_packed = pack(head, head);

            let res = self
                .0
                .head
                .compare_exchange(prev_packed, next_packed, AcqRel, Acquire);

            match res {
                Ok(_) => return n,
                Err(actual) => {
                    let (actual_steal, actual_real) = unpack(actual);

                    assert_ne!(actual_steal, actual_real);

                    prev_packed = actual;
                }
            }
        }
    }
}

impl<T> Clone for Steal<T> {
    fn clone(&self) -> Steal<T> {
        Steal(self.0.clone())
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
        let (_, head) = unpack(self.head.load(Acquire));
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

    pub(super) fn len(&self) -> usize {
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
    unsafe { header.as_ref().queue_next.with(|ptr| *ptr) }
}

fn set_next(header: NonNull<task::Header>, val: Option<NonNull<task::Header>>) {
    unsafe {
        header.as_ref().queue_next.with_mut(|ptr| *ptr = val);
    }
}

/// Split the head value into the real head and the index a stealer is working
/// on.
fn unpack(n: u32) -> (u16, u16) {
    let real = n & u16::max_value() as u32;
    let steal = n >> 16;

    (steal as u16, real as u16)
}

/// Join the two head values
fn pack(steal: u16, real: u16) -> u32 {
    (real as u32) | ((steal as u32) << 16)
}

#[test]
fn test_local_queue_capacity() {
    assert!(LOCAL_QUEUE_CAPACITY - 1 <= u8::max_value() as usize);
}
