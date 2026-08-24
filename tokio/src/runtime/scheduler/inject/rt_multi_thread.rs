use super::{Inject, Pop};

use crate::runtime::task;

use std::sync::atomic::Ordering::Release;

impl<T: 'static> Inject<T> {
    pub(crate) fn is_empty(&self) -> bool {
        self.shared.is_empty()
    }

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
            unsafe { prev.set_queue_next(Some(next)) };
            prev = next;
            counter += 1;
        });

        // Now that the tasks are linked together, insert them into the
        // linked list.
        self.push_batch_inner(first, prev, counter);
    }

    /// Inserts several tasks that have been linked together into the queue.
    ///
    /// The provided head and tail may be the same task. In this case, a
    /// single task is inserted.
    #[inline]
    fn push_batch_inner(&self, batch_head: task::RawTask, batch_tail: task::RawTask, num: usize) {
        debug_assert!(unsafe { batch_tail.get_queue_next().is_none() });

        let mut synced = self.synced.lock();

        if synced.is_closed {
            // Drop the lock before dropping the tasks: dropping a task can
            // run arbitrary user `Drop` code, which may reentrantly acquire
            // this lock by scheduling a task.
            drop(synced);

            let mut curr = Some(batch_head);

            while let Some(task) = curr {
                // safety: `push_batch` took ownership of each task's
                // `Notified` and linked the batch through the tasks'
                // `queue_next` fields; reconstituting the `Notified` here
                // transfers that ownership back.
                curr = unsafe { task.get_queue_next() };

                let _ = unsafe { task::Notified::<T>::from_raw(task) };
            }

            return;
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
        let len = unsafe { self.shared.len.unsync_load() };

        self.shared.len.store(len + num, Release);
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
    #[cfg(feature = "taskdump")]
    pub(crate) fn drain_into(&self, dst: &mut Vec<task::Notified<T>>) {
        let mut synced = self.synced.lock();
        // safety: passing correct `Synced`
        while let Some(task) = unsafe { self.shared.pop(&mut synced) } {
            dst.push(task);
        }
    }
}
