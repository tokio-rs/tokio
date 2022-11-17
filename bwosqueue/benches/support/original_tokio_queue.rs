// This is the original tokio queue, modified slightly to use const generics for a configurable queue size
#![allow(dead_code)]

//! Run-queue structures to support a work-stealing scheduler

use loom::atomic_u16::AtomicU16;
use loom::atomic_u32::AtomicU32;
use loom::unsafe_cell::UnsafeCell;
use std::sync::Arc;

use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};

mod loom;

/// Producer handle. May only be used from a single thread.
pub(crate) struct Local<T: 'static, const N: usize> {
    inner: Arc<Inner<T, { N }>>,
}

/// Consumer handle. May be used from many threads.
pub(crate) struct Steal<T: 'static, const N: usize>(Arc<Inner<T, { N }>>);

struct Inner<T: 'static, const N: usize> {
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
    buffer: Box<[UnsafeCell<MaybeUninit<T>>; N]>,
}

unsafe impl<T, const N: usize> Send for Inner<T, { N }> {}
unsafe impl<T, const N: usize> Sync for Inner<T, { N }> {}

fn make_mask(queue_size: usize) -> usize {
    assert!(queue_size.is_power_of_two());
    queue_size - 1
}

// Constructing the fixed size array directly is very awkward. The only way to
// do it is to repeat `UnsafeCell::new(MaybeUninit::uninit())` 256 times, as
// the contents are not Copy. The trick with defining a const doesn't work for
// generic types.
fn make_fixed_size<T, const N: usize>(buffer: Box<[T]>) -> Box<[T; N]> {
    assert_eq!(buffer.len(), N);

    // safety: We check that the length is correct.
    unsafe { Box::from_raw(Box::into_raw(buffer).cast()) }
}

/// Create a new local run-queue
pub(crate) fn local<T: 'static, const N: usize>() -> (Steal<T, { N }>, Local<T, { N }>) {
    let mut buffer = Vec::with_capacity(N);

    for _ in 0..N {
        buffer.push(UnsafeCell::new(MaybeUninit::uninit()));
    }

    let inner = Arc::new(Inner {
        head: AtomicU32::new(0),
        tail: AtomicU16::new(0),
        buffer: make_fixed_size(buffer.into_boxed_slice()),
    });

    let local = Local {
        inner: inner.clone(),
    };

    let remote = Steal(inner);

    (remote, local)
}

impl<T, const N: usize> Local<T, { N }> {
    /// Returns true if the queue has entries that can be stolen.
    pub(crate) fn is_stealable(&self) -> bool {
        !self.inner.is_empty()
    }

    /// BwoS bench: Exposed this check from steal_into2 as public to use in the benchmark.
    pub(crate) fn has_stealers(&self) -> bool {
        let prev_packed = self.inner.head.load(Acquire);
        let (src_head_steal, src_head_real) = unpack(prev_packed);
        // If these two do not match, another thread is concurrently
        // stealing from the queue.
        src_head_steal != src_head_real
    }

    /// Returns false if there are any entries in the queue
    ///
    /// Separate to is_stealable so that refactors of is_stealable to "protect"
    /// some tasks from stealing won't affect this
    pub(crate) fn has_tasks(&self) -> bool {
        !self.inner.is_empty()
    }

    /// Pushes a task to the back of the local queue, skipping the LIFO slot.
    pub(crate) fn push_back(&mut self, task: T) -> Result<(), T> {
        let tail = loop {
            let head = self.inner.head.load(Acquire);
            let (steal, _real) = unpack(head);

            // safety: this is the **only** thread that updates this cell.
            let tail = unsafe { self.inner.tail.unsync_load() };

            if tail.wrapping_sub(steal) < N as u16 {
                // There is capacity for the task
                break tail;
            } else {
                // Concurrently stealing, this will free up capacity, so only
                // push the task onto the inject queue
                //inject.push(task);
                return Err(task);
            } // JS: remove push_pverflow case for micro benchmark
        };

        // Map the position to a slot index.
        let idx = tail as usize & make_mask(N);

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
        Ok(())
    }

    // /// Moves a batch of tasks into the inject queue.
    // ///
    // /// This will temporarily make some of the tasks unavailable to stealers.
    // /// Once `push_overflow` is done, a notification is sent out, so if other
    // /// workers "missed" some of the tasks during a steal, they will get
    // /// another opportunity.
    // #[inline(never)]
    // fn push_overflow(
    //     &mut self,
    //     task: T,
    //     head: u16,
    //     tail: u16,
    // ) -> Result<(), T> {
    //     /// How many elements are we taking from the local queue.
    //     ///
    //     /// This is one less than the number of tasks pushed to the inject
    //     /// queue as we are also inserting the `task` argument.
    //     const NUM_TASKS_TAKEN: u16 = (LOCAL_QUEUE_CAPACITY / 2) as u16;

    //     assert_eq!(
    //         tail.wrapping_sub(head) as usize,
    //         LOCAL_QUEUE_CAPACITY,
    //         "queue is not full; tail = {}; head = {}",
    //         tail,
    //         head
    //     );

    //     let prev = pack(head, head);

    //     // Claim a bunch of tasks
    //     //
    //     // We are claiming the tasks **before** reading them out of the buffer.
    //     // This is safe because only the **current** thread is able to push new
    //     // tasks.
    //     //
    //     // There isn't really any need for memory ordering... Relaxed would
    //     // work. This is because all tasks are pushed into the queue from the
    //     // current thread (or memory has been acquired if the local queue handle
    //     // moved).
    //     if self
    //         .inner
    //         .head
    //         .compare_exchange(
    //             prev,
    //             pack(
    //                 head.wrapping_add(NUM_TASKS_TAKEN),
    //                 head.wrapping_add(NUM_TASKS_TAKEN),
    //             ),
    //             Release,
    //             Relaxed,
    //         )
    //         .is_err()
    //     {
    //         // We failed to claim the tasks, losing the race. Return out of
    //         // this function and try the full `push` routine again. The queue
    //         // may not be full anymore.
    //         return Err(task);
    //     }

    //     /// An iterator that takes elements out of the run queue.
    //     struct BatchTaskIter<'a, T: 'static> {
    //         buffer: &'a [UnsafeCell<MaybeUninit<task::Notified<T>>>; LOCAL_QUEUE_CAPACITY],
    //         head: u32,
    //         i: u32,
    //     }
    //     impl<'a, T: 'static> Iterator for BatchTaskIter<'a, T> {
    //         type Item = task::Notified<T>;

    //         #[inline]
    //         fn next(&mut self) -> Option<task::Notified<T>> {
    //             if self.i == u32::from(NUM_TASKS_TAKEN) {
    //                 None
    //             } else {
    //                 let i_idx = self.i.wrapping_add(self.head) as usize & MASK;
    //                 let slot = &self.buffer[i_idx];

    //                 // safety: Our CAS from before has assumed exclusive ownership
    //                 // of the task pointers in this range.
    //                 let task = slot.with(|ptr| unsafe { ptr::read((*ptr).as_ptr()) });

    //                 self.i += 1;
    //                 Some(task)
    //             }
    //         }
    //     }

    //     // safety: The CAS above ensures that no consumer will look at these
    //     // values again, and we are the only producer.
    //     let batch_iter = BatchTaskIter {
    //         buffer: &*self.inner.buffer,
    //         head: head as u32,
    //         i: 0,
    //     };
    //     inject.push_batch(batch_iter.chain(std::iter::once(task)));

    //     // Add 1 to factor in the task currently being scheduled.
    //     metrics.incr_overflow_count();

    //     Ok(())
    // }

    /// Pops a task from the local queue.
    pub(crate) fn pop(&mut self) -> Option<T> {
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
                Ok(_) => break real as usize & make_mask(N),
                Err(actual) => head = actual,
            }
        };

        Some(self.inner.buffer[idx].with(|ptr| unsafe { ptr::read(ptr).assume_init() }))
    }
}

impl<const N: usize> Steal<u64, { N }> {
    // BWoS bench: Taken from steal_into2 - Modified to support benchmarking
    // only the steal operation without the additional enqueue into `dst`.
    // Don't steal more than max_steal items, otherwise the stealer will
    // steal 100% of all items and give the consumer in the benchmark no
    // chance, so we can't measure consumer/stealer interference
    pub fn bench_tokio_q_steal(&self, max_steal: u16) -> u16 {
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
            // Bench BWoS steal at most
            let n = core::cmp::min(max_steal, n - n / 2);

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

        assert!(n <= N as u16 / 2, "actual = {}", n);

        let (first, _) = unpack(next_packed);

        // Take all the tasks
        for i in 0..n {
            // Compute the positions
            let src_pos = first.wrapping_add(i);

            // Map to slots
            let src_idx = src_pos as usize & make_mask(N);

            // Read the task
            //
            // safety: We acquired the task with the atomic exchange above.
            let task = self.0.buffer[src_idx].with(|ptr| unsafe { ptr::read((*ptr).as_ptr()) });

            // Use the queue entry so the compiler does not optimize the read away.
            assert_eq!(task, 5);
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

impl<T, const N: usize> Steal<T, { N }> {
    // BWoS bench
    // Based on Local::pop, with modified memory ordering.
    pub(crate) fn bench_tokio_steal_single(&self) -> Option<T> {
        let mut head = self.0.head.load(Acquire);

        let idx = loop {
            let (steal, real) = unpack(head);

            let tail = self.0.tail.load(Acquire);

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
            let res = self.0.head.compare_exchange(head, next, AcqRel, Acquire);

            match res {
                Ok(_) => break real as usize & make_mask(N),
                Err(actual) => head = actual,
            }
        };

        Some(self.0.buffer[idx].with(|ptr| unsafe { ptr::read(ptr).assume_init() }))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Steals half the tasks from self and place them into `dst`.
    pub(crate) fn steal_into(&self, dst: &mut Local<T, N>) -> Option<T> {
        // Safety: the caller is the only thread that mutates `dst.tail` and
        // holds a mutable reference.
        let dst_tail = unsafe { dst.inner.tail.unsync_load() };

        // To the caller, `dst` may **look** empty but still have values
        // contained in the buffer. If another thread is concurrently stealing
        // from `dst` there may not be enough capacity to steal.
        let (steal, _) = unpack(dst.inner.head.load(Acquire));

        if dst_tail.wrapping_sub(steal) > N as u16 / 2 {
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
        let ret_idx = ret_pos as usize & make_mask(N);

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
    fn steal_into2(&self, dst: &mut Local<T, N>, dst_tail: u16) -> u16 {
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

        assert!(n <= N as u16 / 2, "actual = {}", n);

        let (first, _) = unpack(next_packed);

        // Take all the tasks
        for i in 0..n {
            // Compute the positions
            let src_pos = first.wrapping_add(i);
            let dst_pos = dst_tail.wrapping_add(i);

            // Map to slots
            let src_idx = src_pos as usize & make_mask(N);
            let dst_idx = dst_pos as usize & make_mask(N);

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

impl<T, const N: usize> Clone for Steal<T, { N }> {
    fn clone(&self) -> Steal<T, N> {
        Steal(self.0.clone())
    }
}

impl<T, const N: usize> Drop for Local<T, { N }> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.pop().is_none(), "queue not empty");
        }
    }
}

impl<T, const N: usize> Inner<T, { N }> {
    fn len(&self) -> u16 {
        let (_, head) = unpack(self.head.load(Acquire));
        let tail = self.tail.load(Acquire);

        tail.wrapping_sub(head)
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Split the head value into the real head and the index a stealer is working
/// on.
fn unpack(n: u32) -> (u16, u16) {
    let real = n & u16::MAX as u32;
    let steal = n >> 16;

    (steal as u16, real as u16)
}

/// Join the two head values
fn pack(steal: u16, real: u16) -> u32 {
    (real as u32) | ((steal as u32) << 16)
}

#[test]
fn test_local_queue_capacity() {
    assert!(LOCAL_QUEUE_CAPACITY - 1 <= u8::MAX as usize);
}
