//cfg_rt_multi_thread_bwos! {
pub(crate) mod bwosq;
//}

pub(crate) mod tokioq;

use crate::runtime::builder::MultiThreadFlavor;
use crate::runtime::task::Inject;
use crate::runtime::{task, MetricsBatch};

pub(crate) fn local<T: 'static>(
    flavor: MultiThreadFlavor,
) -> (
    Box<dyn Stealer<T> + Send + Sync>,
    Box<dyn Owner<T> + Send + Sync>,
) {
    match flavor {
        MultiThreadFlavor::Default => tokioq::local(),
        //#[cfg(all(tokio_unstable, feature = "bwos"))]
        MultiThreadFlavor::Bwos => bwosq::local(),
    }
}

pub(crate) trait Owner<T: 'static>: Send + Sync {
    /// Returns true if the queue has entries that can be stolen.
    fn is_stealable(&self) -> bool;

    /// Returns the maximum capacity of the underlying queue.
    fn max_capacity(&self) -> usize;

    /// Returns a tuple with the lower bound and an Option for the upper bound of remaining
    /// slots for enqueuing in the queue.
    fn remaining_slots_hint(&self) -> (u16, Option<u16>);

    /// Returns true if there are entries in the queue.
    fn has_tasks(&self) -> bool;

    /// Pushes a batch of tasks to the back of the queue. All tasks must fit in
    /// the local queue.
    ///
    /// # Panics
    ///
    /// The method panics if there is not enough capacity to fit in the queue.
    fn push_back(&mut self, tasks: Box<dyn ExactSizeIterator<Item = task::Notified<T>> + '_>);

    /// Pushes a task to the back of the local queue, if there is not enough
    /// capacity in the queue, this triggers the overflow operation.
    ///
    /// When the queue overflows, half of the current contents of the queue is
    /// moved to the given Injection queue. This frees up capacity for more
    /// tasks to be pushed into the local queue.    
    fn push_back_or_overflow(
        &mut self,
        task: task::Notified<T>,
        inject: &Inject<T>,
        metrics: &mut MetricsBatch,
    );

    /// Push a batch of tasks to the back of the local queue
    ///
    /// # Safety:
    ///
    /// The caller must ensure that the queue has enough capacity to accept
    /// all tasks, e.g. by calling `can_enqueue` beforehand.
    unsafe fn push_back_unchecked(
        &mut self,
        tasks: Box<dyn ExactSizeIterator<Item = task::Notified<T>> + '_>,
    );

    /// Pop one task from the front of the queue.
    fn pop(&mut self) -> Option<task::Notified<T>>;
}

pub(crate) trait Stealer<T>: Send + Sync {
    /// Returns true if the queue is empty
    ///
    /// This function _must_ be accurate and is intended to be used
    /// only in non-performance critical settings.
    fn is_empty(&self) -> bool;

    /// Steals half the tasks from self and place them into `dst`.
    fn steal_into(
        &self,
        dst: &mut dyn Owner<T>,
        dst_metrics: &mut MetricsBatch,
    ) -> Option<task::Notified<T>>;

    cfg_metrics! {
        /// Number of tasks in the queue.
        #[cfg(feature = "stats")]
        fn len(&self) -> usize;
    }
}
