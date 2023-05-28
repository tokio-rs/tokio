use std::convert::TryInto;

use crate::runtime::scheduler::multi_thread::queue::Owner as OwnerTrait;
use crate::runtime::task::{self, Inject, Notified};
use crate::runtime::MetricsBatch;

mod bwosqueue;

// todo: Discuss using const generics or runtime values. Benchmark performance difference.
const NUM_BLOCKS: usize = 8;
const ELEMENTS_PER_BLOCK: usize = 32;

/// Producer handle. May only be used from a single thread.
pub(crate) struct Local<T: 'static> {
    inner: bwosqueue::Owner<task::Notified<T>, NUM_BLOCKS, ELEMENTS_PER_BLOCK>,
}

/// Consumer handle. May be used from many threads.
pub(crate) struct Steal<T: 'static>(
    bwosqueue::Stealer<task::Notified<T>, NUM_BLOCKS, ELEMENTS_PER_BLOCK>,
);

/// Create a new local run-queue
pub(crate) fn local<T: 'static>() -> (
    Box<dyn super::Stealer<T> + Send + Sync>,
    Box<dyn super::Owner<T> + Send + Sync>,
) {
    let (owner, stealer) = bwosqueue::new::<task::Notified<T>, NUM_BLOCKS, ELEMENTS_PER_BLOCK>();

    let local = Local { inner: owner };

    let remote = Steal(stealer);

    (Box::new(remote), Box::new(local))
}

impl<T> super::Owner<T> for Local<T> {
    /// Returns true if the queue has entries that can be stolen.
    fn is_stealable(&self) -> bool {
        self.inner.has_stealable_entries()
    }

    fn max_capacity(&self) -> usize {
        self::ELEMENTS_PER_BLOCK * self::NUM_BLOCKS
    }

    /// Returns true if there are entries in the queue.
    fn has_tasks(&self) -> bool {
        self.inner.can_consume()
    }

    /// Pushes a task to the back of the local queue, skipping the LIFO slot.
    fn push_back_or_overflow(
        &mut self,
        task: task::Notified<T>,
        inject: &Inject<T>,
        metrics: &mut MetricsBatch,
    ) {
        if let Err(t) = self.inner.enqueue(task) {
            if self.inner.next_block_has_stealers() {
                inject.push(t);
            } else {
                // push overflow of old queue
                if let Some(block_iter) = self.inner.dequeue_block() {
                    inject.push_batch(block_iter.chain(std::iter::once(t)))
                } else {
                    inject.push(t)
                }
            }
            metrics.incr_overflow_count();
        };
    }

    fn push_back(&mut self, tasks: Box<dyn ExactSizeIterator<Item = Notified<T>> + '_>) {
        let len = tasks.len();
        let min_capacity = self.inner.min_remaining_slots();
        assert!(len <= min_capacity);
        // SAFETU: We checked the capacity of the queue is sufficient before enqueuing.
        unsafe {
            self.inner.enqueue_batch_unchecked(tasks);
        }
    }

    unsafe fn push_back_unchecked(
        &mut self,
        tasks: Box<dyn ExactSizeIterator<Item = Notified<T>> + '_>,
    ) {
        let _num_enqueued = self.inner.enqueue_batch_unchecked(tasks);
    }

    fn remaining_slots_hint(&self) -> (u16, Option<u16>) {
        let min_slots = self.inner.min_remaining_slots();
        debug_assert!(min_slots <= u16::MAX.into());
        // Note: If we do change from a linked list of blocks to an array of blocks,
        // we may be able to quickly calculate an approximate upper bound based
        // on the consumer cache _index_.
        (min_slots as u16, None)
    }

    fn pop(&mut self) -> Option<task::Notified<T>> {
        self.inner.dequeue()
    }
}

impl<T> Drop for Local<T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.pop().is_none(), "queue not empty");
        }
    }
}

impl<T> super::Stealer<T> for Steal<T> {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Steals one block from self and place them into `dst`.
    fn steal_into(
        &self,
        dst: &mut dyn OwnerTrait<T>,
        dst_metrics: &mut MetricsBatch,
    ) -> Option<task::Notified<T>> {
        // In the rare case that the `dst` queue is at the same time also full, because the
        // producer is blocked waiting on a stealer we only attempt to steal a single task
        if dst.remaining_slots_hint().0 < ELEMENTS_PER_BLOCK as u16 {
            dst_metrics.incr_steal_count(1);
            dst_metrics.incr_steal_operations();
            // We could evaluate stealing exactly the amount of remaining slots + 1.
            return self.0.steal();
        }

        if let Some(mut stolen_tasks) = self.0.steal_block() {
            let num_stolen = stolen_tasks.len();
            let first = stolen_tasks.next();
            debug_assert!(first.is_some());
            unsafe { dst.push_back_unchecked(Box::new(stolen_tasks)) }
            dst_metrics.incr_steal_count(num_stolen.try_into().unwrap());
            dst_metrics.incr_steal_operations();
            first
        } else {
            None
        }
    }

    cfg_metrics! {
        /// Approximate queue length
        fn len(&self) -> usize {
            self.0.estimated_queue_entries()
        }

    }
}

impl<T> Clone for Steal<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
