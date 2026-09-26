use super::Handle;

use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::{Arc, Mutex};
use crate::runtime::task;
use crate::util::cacheline::CachePadded;

use std::collections::VecDeque;
use std::sync::atomic::Ordering::{Acquire, Release};

pub(super) const NO_PARTITION: usize = usize::MAX;

/// Per-LLC queues are sharded to avoid a single runtime-wide injection lock.
/// Entries within each LLC queue are scheduled in FIFO order.
pub(super) type LlcQueues = Queues<task::Notified<Arc<Handle>>>;

pub(super) struct Queues<T> {
    partitions: Box<[CachePadded<LlcQueue<T>>]>,
    non_empty: Box<[AtomicUsize]>,
    workers: Box<[AtomicUsize]>,
    worker_members: Box<[Box<[AtomicUsize]>]>,
}

struct LlcQueue<T> {
    len: AtomicUsize,
    state: Mutex<State<T>>,
}

struct State<T> {
    closed: bool,
    entries: VecDeque<T>,
}

impl<T> Queues<T> {
    pub(super) fn new(partitions: usize, worker_count: usize) -> Self {
        let partitions: Box<[CachePadded<LlcQueue<T>>]> = (0..partitions)
            .map(|_| {
                CachePadded::new(LlcQueue {
                    len: AtomicUsize::new(0),
                    state: Mutex::new(State {
                        closed: false,
                        entries: VecDeque::new(),
                    }),
                })
            })
            .collect();
        let bits = usize::BITS as usize;
        let non_empty = (0..(partitions.len() + bits - 1) / bits)
            .map(|_| AtomicUsize::new(0))
            .collect();
        let workers = (0..partitions.len()).map(|_| AtomicUsize::new(0)).collect();
        let worker_words = (worker_count + bits - 1) / bits;
        let worker_members = (0..partitions.len())
            .map(|_| (0..worker_words).map(|_| AtomicUsize::new(0)).collect())
            .collect();
        Self {
            partitions,
            non_empty,
            workers,
            worker_members,
        }
    }

    pub(super) fn len(&self, partition: usize) -> usize {
        self.partitions[partition].len.load(Acquire)
    }

    pub(super) fn partition_count(&self) -> usize {
        self.partitions.len()
    }

    pub(super) fn worker_count(&self, partition: usize) -> usize {
        self.workers[partition].load(Acquire)
    }

    pub(super) fn update_worker(
        &self,
        worker: usize,
        previous: Option<usize>,
        next: Option<usize>,
    ) {
        if previous == next {
            return;
        }
        let bits = usize::BITS as usize;
        let word = worker / bits;
        let bit = 1 << (worker % bits);
        if let Some(next) = next {
            self.worker_members[next][word].fetch_or(bit, Release);
            self.workers[next].fetch_add(1, Release);
        }
        if let Some(previous) = previous {
            self.worker_members[previous][word].fetch_and(!bit, Release);
            let count = self.workers[previous].fetch_sub(1, Release);
            debug_assert!(count > 0);
        }
    }

    pub(super) fn find_worker<R>(
        &self,
        partition: usize,
        start: usize,
        mut f: impl FnMut(usize) -> Option<R>,
    ) -> Option<R> {
        let bits = usize::BITS as usize;
        let members = &self.worker_members[partition];
        let start_word = (start / bits) % members.len();
        let start_bit = start % bits;

        for word_offset in 0..members.len() {
            let word_index = (start_word + word_offset) % members.len();
            let rotation = if word_offset == 0 { start_bit } else { 0 };
            let mut candidates = members[word_index]
                .load(Acquire)
                .rotate_right(rotation as u32);

            while candidates != 0 {
                let rotated_bit = candidates.trailing_zeros() as usize;
                candidates &= candidates - 1;
                let bit = (rotated_bit + rotation) % bits;
                let worker = word_index * bits + bit;
                if let Some(value) = f(worker) {
                    return Some(value);
                }
            }
        }
        None
    }

    pub(super) fn is_empty(&self, partition: usize) -> bool {
        self.len(partition) == 0
    }

    pub(super) fn all_empty(&self) -> bool {
        self.non_empty.iter().all(|word| word.load(Acquire) == 0)
    }

    pub(super) fn push(&self, partition: usize, task: T) {
        let queue = &self.partitions[partition];
        let mut state = queue.state.lock();
        if state.closed {
            return;
        }

        let became_non_empty = state.entries.is_empty();
        state.entries.push_back(task);
        queue.len.store(state.entries.len(), Release);
        if became_non_empty {
            self.mark_non_empty(partition);
        }
    }

    pub(super) fn push_batch<I>(&self, partition: usize, tasks: I)
    where
        I: Iterator<Item = T>,
    {
        let queue = &self.partitions[partition];
        let mut state = queue.state.lock();
        if state.closed {
            return;
        }

        let previous_len = state.entries.len();
        state.entries.extend(tasks);
        let len = state.entries.len();
        queue.len.store(len, Release);
        if previous_len == 0 && len != 0 {
            self.mark_non_empty(partition);
        }
    }

    pub(super) fn pop(&self, partition: usize) -> Option<T> {
        let queue = &self.partitions[partition];
        if queue.len.load(Acquire) == 0 {
            return None;
        }

        let mut state = queue.state.lock();
        let task = state.entries.pop_front()?;
        queue.len.store(state.entries.len(), Release);
        if state.entries.is_empty() {
            self.clear_non_empty(partition);
        }
        Some(task)
    }

    pub(super) fn pop_n<R>(
        &self,
        partition: usize,
        count: usize,
        f: impl FnOnce(Pop<'_, T>) -> R,
    ) -> R {
        let queue = &self.partitions[partition];
        let mut state = queue.state.lock();
        let count = count.min(state.entries.len());
        let result = f(Pop {
            entries: &mut state.entries,
            remaining: count,
        });
        queue.len.store(state.entries.len(), Release);
        if state.entries.is_empty() {
            self.clear_non_empty(partition);
        }
        result
    }

    /// Pops from a non-empty partition other than `home`, probing at most
    /// `max_probes` queues. The presence bitmap makes the scan proportional to
    /// the number of machine words rather than the number of LLCs.
    pub(super) fn pop_other_where(
        &self,
        home: usize,
        start: usize,
        max_probes: usize,
        mut matches: impl FnMut(usize) -> bool,
    ) -> Option<T> {
        let bits = usize::BITS as usize;
        let start_word = (start / bits) % self.non_empty.len();
        let start_bit = start % bits;
        let mut probes = 0;

        for word_offset in 0..self.non_empty.len() {
            let word_index = (start_word + word_offset) % self.non_empty.len();
            let rotation = if word_offset == 0 { start_bit } else { 0 };
            let mut candidates = self.non_empty[word_index].load(Acquire);

            if home / bits == word_index {
                candidates &= !(1 << (home % bits));
            }

            candidates = candidates.rotate_right(rotation as u32);
            while candidates != 0 && probes < max_probes {
                let rotated_bit = candidates.trailing_zeros() as usize;
                candidates &= candidates - 1;
                let bit = (rotated_bit + rotation) % bits;
                let partition = word_index * bits + bit;
                if partition < self.partitions.len() && matches(partition) {
                    probes += 1;
                    if let Some(task) = self.pop(partition) {
                        return Some(task);
                    }
                }
            }

            if probes == max_probes {
                break;
            }
        }
        None
    }

    pub(super) fn close(&self) {
        for queue in &self.partitions {
            queue.state.lock().closed = true;
        }
    }

    #[cfg(all(tokio_unstable, feature = "taskdump"))]
    pub(super) fn drain_into(&self, dst: &mut Vec<T>) {
        for (partition, queue) in self.partitions.iter().enumerate() {
            let mut state = queue.state.lock();
            while let Some(task) = state.entries.pop_front() {
                dst.push(task);
            }
            queue.len.store(0, Release);
            self.clear_non_empty(partition);
        }
    }

    fn mark_non_empty(&self, partition: usize) {
        let bits = usize::BITS as usize;
        let word = &self.non_empty[partition / bits];
        let bit = 1 << (partition % bits);
        word.fetch_or(bit, Release);
    }

    fn clear_non_empty(&self, partition: usize) {
        let bits = usize::BITS as usize;
        let word = &self.non_empty[partition / bits];
        word.fetch_and(!(1 << (partition % bits)), Release);
    }
}

pub(super) struct Pop<'a, T> {
    entries: &'a mut VecDeque<T>,
    remaining: usize,
}

#[cfg(all(test, loom))]
pub(crate) fn model_two_partition_queue_races() {
    loom::model(|| {
        let queues = Arc::new(Queues::<usize>::new(2, 2));
        queues.update_worker(0, None, Some(0));
        queues.update_worker(1, None, Some(1));
        queues.push(0, 0);
        queues.push(1, 1);

        let first = queues.clone();
        let first = loom::thread::spawn(move || {
            assert_eq!(first.pop(0), Some(0));
            first.push(1, 2);
        });

        let second = queues.clone();
        let second = loom::thread::spawn(move || {
            assert_eq!(second.pop(1), Some(1));
            second.push(0, 3);
        });

        first.join().unwrap();
        second.join().unwrap();
        assert_eq!(queues.pop(0), Some(3));
        assert_eq!(queues.pop(1), Some(2));
        assert!(queues.all_empty());

        let first = queues.clone();
        let first = loom::thread::spawn(move || first.push(0, 4));
        let second = queues.clone();
        let second = loom::thread::spawn(move || second.push(1, 5));
        queues.close();
        first.join().unwrap();
        second.join().unwrap();
        assert!(matches!(queues.pop(0), None | Some(4)));
        assert!(matches!(queues.pop(1), None | Some(5)));
        assert!(queues.all_empty());

        queues.push(0, 6);
        queues.push(1, 7);
        assert!(queues.all_empty());
    });
}

impl<T> Iterator for Pop<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        Some(self.entries.pop_front().expect("LLC queue length changed"))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<T> ExactSizeIterator for Pop<'_, T> {}

impl<T> Drop for Pop<'_, T> {
    fn drop(&mut self) {
        // Keep the queue's accounting exact even if the consumer intentionally
        // stops early.
        while self.next().is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::{LlcQueues, State};
    use std::collections::VecDeque;

    #[test]
    fn queue_is_fifo() {
        let mut state = State {
            closed: false,
            entries: VecDeque::new(),
        };
        state.entries.push_back('a');
        state.entries.push_back('b');
        state.entries.push_back('c');

        assert_eq!(state.entries.pop_front(), Some('a'));
        assert_eq!(state.entries.pop_front(), Some('b'));
        assert_eq!(state.entries.pop_front(), Some('c'));
    }

    #[test]
    fn worker_members_follow_partition_changes() {
        let queues = LlcQueues::new(3, 130);
        queues.update_worker(0, None, Some(1));
        queues.update_worker(64, None, Some(1));
        queues.update_worker(129, None, Some(1));

        let mut members = Vec::new();
        queues.find_worker(1, 63, |worker| {
            members.push(worker);
            None::<()>
        });
        members.sort_unstable();
        assert_eq!(members, [0, 64, 129]);
        assert_eq!(queues.worker_count(1), 3);

        queues.update_worker(64, Some(1), Some(2));
        assert_eq!(queues.worker_count(1), 2);
        assert_eq!(queues.worker_count(2), 1);
        assert_eq!(queues.find_worker(1, 64, Some), Some(129));
        assert_eq!(queues.find_worker(2, 0, Some), Some(64));
    }
}
