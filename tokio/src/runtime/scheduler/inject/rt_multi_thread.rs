use super::{Inject, Pop, ShardedInject, Shared, Synced};

use crate::loom::sync::Mutex;
use crate::runtime::task;

use std::sync::atomic::Ordering::Release;

impl<T: 'static> Shared<T> {
    /// Pushes several values into the queue, taking `synced`'s lock only
    /// after linking them together. Returns `true` if the queue was empty
    /// and became non-empty.
    ///
    /// # Safety
    ///
    /// `synced` must be the `Synced` instance returned with this `Shared`
    /// by `Shared::new`.
    #[inline]
    pub(super) unsafe fn push_batch<I>(&self, synced: &Mutex<Synced>, mut iter: I) -> bool
    where
        I: Iterator<Item = task::Notified<T>>,
    {
        let first = match iter.next() {
            Some(first) => first.into_raw(),
            None => return false,
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
            unsafe { prev.set_queue_next(Some(next)) };
            prev = next;
            counter += 1;
        });

        // Now that the tasks are linked together, insert them into the
        // linked list.
        //
        // safety: the batch was linked just above from `Notified`s this
        // function took ownership of, satisfying both obligations; `synced`
        // is passed through from this function's own contract.
        unsafe { self.push_batch_inner(synced, first, prev, counter) }
    }

    /// Inserts several tasks that have been linked together into the queue.
    /// Returns `true` if the queue was empty and became non-empty.
    ///
    /// The provided head and tail may be the same task. In this case, a
    /// single task is inserted.
    ///
    /// # Safety
    ///
    /// `synced` must be the `Synced` instance returned with this `Shared`
    /// by `Shared::new`. The caller must own the `Notified` for each of the
    /// `num` tasks, and the tasks must be linked from `batch_head` to
    /// `batch_tail` through their `queue_next` fields, with `batch_tail`'s
    /// `queue_next` unset.
    #[inline]
    unsafe fn push_batch_inner(
        &self,
        synced: &Mutex<Synced>,
        batch_head: task::RawTask,
        batch_tail: task::RawTask,
        num: usize,
    ) -> bool {
        debug_assert!(unsafe { batch_tail.get_queue_next().is_none() });

        let mut synced = synced.lock();

        if synced.is_closed {
            // Drop the lock before dropping the tasks: dropping a task can
            // run arbitrary user `Drop` code, which may reentrantly acquire
            // this lock by scheduling a task.
            drop(synced);

            let mut curr = Some(batch_head);

            while let Some(task) = curr {
                // safety: per this function's contract, the caller owns each
                // task's `Notified` and linked the batch through `queue_next`;
                // reconstituting the `Notified` here takes that ownership.
                curr = unsafe { task.get_queue_next() };

                let _ = unsafe { task::Notified::<T>::from_raw(task) };
            }

            return false;
        }

        if let Some(tail) = synced.tail {
            unsafe {
                tail.set_queue_next(Some(batch_head));
            }
        } else {
            synced.head = Some(batch_head);
        }

        synced.tail = Some(batch_tail);

        // Increment the count.
        //
        // safety: All updates to the len atomic are guarded by the mutex. As
        // such, a non-atomic load followed by a store is safe.
        let len = unsafe { self.len.unsync_load() };

        self.len.store(len + num, Release);

        len == 0
    }
}

/// The multi-thread scheduler's inject queue. Each variant owns its queue and
/// lock topology.
pub(crate) enum InjectQueue<T: 'static> {
    /// A single queue behind a single mutex.
    Locked(Inject<T>),

    /// Several independently-locked queue shards.
    Sharded(ShardedInject<T>),
}

impl<T: 'static> InjectQueue<T> {
    pub(crate) fn new(sharded: bool, num_workers: usize) -> InjectQueue<T> {
        if sharded {
            InjectQueue::Sharded(ShardedInject::new(num_workers))
        } else {
            InjectQueue::Locked(Inject::new())
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        match self {
            InjectQueue::Locked(q) => q.is_empty(),
            InjectQueue::Sharded(q) => q.is_empty(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            InjectQueue::Locked(q) => q.len(),
            InjectQueue::Sharded(q) => q.len(),
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        match self {
            InjectQueue::Locked(q) => q.is_closed(),
            InjectQueue::Sharded(q) => q.is_closed(),
        }
    }

    /// Closes the queue, returns `true` if the queue was open when the
    /// transition was made.
    pub(crate) fn close(&self) -> bool {
        match self {
            InjectQueue::Locked(q) => q.close(),
            InjectQueue::Sharded(q) => q.close(),
        }
    }

    /// Pushes a value into the queue.
    ///
    /// This does nothing if the queue is closed.
    pub(crate) fn push(&self, task: task::Notified<T>) {
        match self {
            InjectQueue::Locked(q) => q.push(task),
            InjectQueue::Sharded(q) => q.push(task),
        }
    }

    pub(crate) fn pop(&self) -> Option<task::Notified<T>> {
        match self {
            InjectQueue::Locked(q) => q.pop(),
            InjectQueue::Sharded(q) => q.pop(),
        }
    }

    /// Pushes several values into the queue.
    ///
    /// This does nothing if the queue is closed.
    pub(crate) fn push_batch<I>(&self, iter: I)
    where
        I: Iterator<Item = task::Notified<T>>,
    {
        match self {
            InjectQueue::Locked(q) => q.push_batch(iter),
            InjectQueue::Sharded(q) => q.push_batch(iter),
        }
    }

    /// Pops up to `n` values from the queue. `f` is called with an iterator
    /// over each batch of popped values; any values `f` does not consume are
    /// removed from the queue and dropped. `f` may be called zero, one, or
    /// (for the sharded queue) several times.
    pub(crate) fn pop_n(&self, n: usize, mut f: impl FnMut(Pop<'_, T>)) {
        match self {
            InjectQueue::Locked(q) => q.pop_n(n, &mut f),
            InjectQueue::Sharded(q) => q.pop_n(n, f),
        }
    }

    /// Pops every task from the queue into `dst`.
    #[cfg(all(tokio_unstable, feature = "taskdump"))]
    pub(crate) fn drain_into(&self, dst: &mut Vec<task::Notified<T>>) {
        match self {
            InjectQueue::Locked(q) => q.drain_into(dst),
            InjectQueue::Sharded(q) => q.drain_into(dst),
        }
    }
}

impl<T: 'static> Inject<T> {
    pub(crate) fn is_empty(&self) -> bool {
        self.shared.is_empty()
    }

    /// Pushes several values into the queue.
    #[inline]
    pub(crate) fn push_batch<I>(&self, iter: I)
    where
        I: Iterator<Item = task::Notified<T>>,
    {
        // safety: `synced` is the `Synced` this `Shared` was created with
        unsafe { self.shared.push_batch(&self.synced, iter) };
    }

    /// Pops up to `n` values from the queue, passing an iterator over them to
    /// `f`. The queue lock is held while `f` runs, so any values `f` does not
    /// consume are removed from the queue and dropped before the lock is
    /// released.
    pub(crate) fn pop_n<R>(&self, n: usize, f: impl FnOnce(Pop<'_, T>) -> R) -> R {
        let mut synced = self.synced.lock();
        // safety: passing correct `Synced`
        f(unsafe { self.shared.pop_n(&mut synced, n) })
    }

    /// Pops every task from the queue into `dst`, holding the queue lock for
    /// the entire drain so it is atomic with respect to concurrent pushes.
    #[cfg(all(tokio_unstable, feature = "taskdump"))]
    pub(crate) fn drain_into(&self, dst: &mut Vec<task::Notified<T>>) {
        let mut synced = self.synced.lock();
        // safety: passing correct `Synced`
        while let Some(task) = unsafe { self.shared.pop(&mut synced) } {
            dst.push(task);
        }
    }
}
