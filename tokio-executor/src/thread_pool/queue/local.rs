use crate::loom::cell::{CausalCell, CausalCheck};
use crate::loom::sync::atomic::{self, AtomicU32};
use crate::task::Task;
use crate::thread_pool::queue::global;
use crate::thread_pool::LOCAL_QUEUE_CAPACITY;

use std::fmt;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::Ordering::{Acquire, Release};

pub(super) struct Queue<T: 'static> {
    /// Concurrently updated by many threads.
    head: AtomicU32,

    /// Only updated by producer thread but read by many threads.
    tail: AtomicU32,

    /// Elements
    buffer: Box<[CausalCell<MaybeUninit<Task<T>>>]>,
}

const MASK: usize = LOCAL_QUEUE_CAPACITY - 1;

impl<T: 'static> Queue<T> {
    pub(super) fn new() -> Queue<T> {
        debug_assert!(LOCAL_QUEUE_CAPACITY >= 2 && LOCAL_QUEUE_CAPACITY.is_power_of_two());

        let mut buffer = Vec::with_capacity(LOCAL_QUEUE_CAPACITY);

        for _ in 0..LOCAL_QUEUE_CAPACITY {
            buffer.push(CausalCell::new(MaybeUninit::uninit()));
        }

        Queue {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            buffer: buffer.into(),
        }
    }
}

impl<T> Queue<T> {
    /// Push a task onto the local queue.
    ///
    /// This **must** be called by the producer thread.
    pub(super) unsafe fn push(&self, mut task: Task<T>, global: &global::Queue<T>) {
        loop {
            let head = self.head.load(Acquire);

            // safety: this is the **only** thread that updates this cell.
            let tail = self.tail.unsync_load();

            if tail.wrapping_sub(head) < LOCAL_QUEUE_CAPACITY as u32 {
                // Map the position to a slot index.
                let idx = tail as usize & MASK;

                self.buffer[idx].with_mut(|ptr| {
                    // Write the task to the slot
                    ptr::write((*ptr).as_mut_ptr(), task);
                });

                // Make the task available
                self.tail.store(tail.wrapping_add(1), Release);

                return;
            }

            // The local buffer is full. Push a batch of work to the global
            // queue.
            match self.push_overflow(task, head, tail, global) {
                Ok(_) => return,
                // Lost the race, try again
                Err(v) => task = v,
            }

            atomic::spin_loop_hint();
        }
    }

    /// Move a batch of tasks into the global queue.
    ///
    /// This will temporarily make some of the tasks unavailable to stealers.
    /// Once `push_overflow` is done, a notification is sent out, so if other
    /// workers "missed" some of the tasks during a steal, they will get
    /// another opportunity.
    #[inline(never)]
    unsafe fn push_overflow(
        &self,
        task: Task<T>,
        head: u32,
        tail: u32,
        global: &global::Queue<T>,
    ) -> Result<(), Task<T>> {
        const BATCH_LEN: usize = LOCAL_QUEUE_CAPACITY / 2 + 1;

        let n = tail.wrapping_sub(head) / 2;
        assert_eq!(n as usize, LOCAL_QUEUE_CAPACITY / 2, "queue is not full");

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
        let actual = self.head.compare_and_swap(head, head + n, Release);
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
                task.header() as *const _
            } else {
                self.buffer[j_idx].with(|ptr| {
                    let value = (*ptr).as_ptr();
                    (*value).header() as *const _
                })
            };

            self.buffer[i_idx].with_mut(|ptr| {
                let ptr = (*ptr).as_ptr();
                debug_assert!((*(*ptr).header().queue_next.get()).is_null());
                *(*ptr).header().queue_next.get() = next;
            });
        }

        let head = self.buffer[head as usize & MASK].with(|ptr| ptr::read((*ptr).as_ptr()));

        // Push the tasks onto the global queue
        global.push_batch(head, task, BATCH_LEN);

        Ok(())
    }

    /// Pop a task from the local queue.
    ///
    /// This **must** be called by the producer thread
    pub(super) unsafe fn pop(&self) -> Option<Task<T>> {
        loop {
            let head = self.head.load(Acquire);

            // safety: this is the **only** thread that updates this cell.
            let tail = self.tail.unsync_load();

            if head == tail {
                // queue is empty
                return None;
            }

            // Map the head position to a slot index.
            let idx = head as usize & MASK;

            let task = self.buffer[idx].with(|ptr| {
                // Tentatively read the task at the head position. Note that we
                // have not yet claimed the task.
                //
                ptr::read(ptr)
            });

            // Attempt to claim the task read above.
            let actual = self
                .head
                .compare_and_swap(head, head.wrapping_add(1), Release);

            if actual == head {
                return Some(task.assume_init());
            }

            atomic::spin_loop_hint();
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        let head = self.head.load(Acquire);
        let tail = self.tail.load(Acquire);

        head == tail
    }

    /// Steal half the tasks from self and place them into `dst`.
    pub(super) unsafe fn steal(&self, dst: &Queue<T>) -> Option<Task<T>> {
        let dst_tail = dst.tail.unsync_load();

        // Steal the tasks into `dst`'s buffer. This does not yet expose the
        // tasks in `dst`.
        let mut n = self.steal2(dst, dst_tail);

        if n == 0 {
            // No tasks were stolen
            return None;
        }

        // We are returning a task here
        n -= 1;

        let ret_pos = dst_tail.wrapping_add(n);
        let ret_idx = ret_pos as usize & MASK;

        let ret = dst.buffer[ret_idx].with(|ptr| ptr::read((*ptr).as_ptr()));

        if n == 0 {
            // The `dst` queue is empty, but a single task was stolen
            return Some(ret);
        }

        // Synchronize with stealers
        let dst_head = dst.head.load(Acquire);

        assert!(dst_tail.wrapping_sub(dst_head) + n <= LOCAL_QUEUE_CAPACITY as u32);

        // Make the stolen items available to consumers
        dst.tail.store(dst_tail.wrapping_add(n), Release);

        Some(ret)
    }

    unsafe fn steal2(&self, dst: &Queue<T>, dst_tail: u32) -> u32 {
        loop {
            let src_head = self.head.load(Acquire);
            let src_tail = self.tail.load(Acquire);

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
                let (task, ch) =
                    self.buffer[src_idx].with_deferred(|ptr| ptr::read((*ptr).as_ptr()));

                check.join(ch);

                // Write the task to the new slot
                dst.buffer[dst_idx].with_mut(|ptr| ptr::write((*ptr).as_mut_ptr(), task));
            }

            // Claim all of those tasks!
            let actual = self
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

impl<T> fmt::Debug for Queue<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("local::Queue")
            .field("head", &self.head)
            .field("tail", &self.tail)
            .field("buffer", &"[...]")
            .finish()
    }
}
