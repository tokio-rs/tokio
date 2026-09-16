use super::Handle;

use crate::loom::sync::{Arc, Mutex};
use crate::loom::sync::atomic::AtomicUsize;
use crate::runtime::task;
use crate::util::cacheline::CachePadded;

use std::collections::VecDeque;
use std::sync::atomic::Ordering::{Acquire, Release};

pub(super) const NO_PARTITION: usize = usize::MAX;

/// Per-LLC queues are sharded to avoid a single runtime-wide injection lock.
/// Entries within each LLC queue are scheduled in FIFO order.
pub(super) struct LlcQueues {
    partitions: Box<[CachePadded<LlcQueue>]>,
    non_empty: Box<[AtomicUsize]>,
    workers: Box<[AtomicUsize]>,
}

struct LlcQueue {
    len: AtomicUsize,
    state: Mutex<State<task::Notified<Arc<Handle>>>>,
}

struct State<T> {
    closed: bool,
    entries: VecDeque<T>,
}

impl LlcQueues {
    pub(super) fn new(partitions: usize) -> Self {
        let partitions: Box<[CachePadded<LlcQueue>]> = (0..partitions)
            .map(|_| CachePadded::new(LlcQueue {
                len: AtomicUsize::new(0),
                state: Mutex::new(State {
                    closed: false,
                    entries: VecDeque::new(),
                }),
            }))
            .collect();
        let bits = usize::BITS as usize;
        let non_empty = (0..(partitions.len() + bits - 1) / bits)
            .map(|_| AtomicUsize::new(0))
            .collect();
        let workers = (0..partitions.len())
            .map(|_| AtomicUsize::new(0))
            .collect();
        Self {
            partitions,
            non_empty,
            workers,
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

    pub(super) fn update_worker(&self, previous: Option<usize>, next: Option<usize>) {
        if previous == next {
            return;
        }
        if let Some(next) = next {
            self.workers[next].fetch_add(1, Release);
        }
        if let Some(previous) = previous {
            let count = self.workers[previous].fetch_sub(1, Release);
            debug_assert!(count > 0);
        }
    }

    pub(super) fn is_empty(&self, partition: usize) -> bool {
        self.len(partition) == 0
    }

    pub(super) fn all_empty(&self) -> bool {
        self.non_empty.iter().all(|word| word.load(Acquire) == 0)
    }

    pub(super) fn push(&self, partition: usize, task: task::Notified<Arc<Handle>>) {
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

    pub(super) fn pop(&self, partition: usize) -> Option<task::Notified<Arc<Handle>>> {
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
        f: impl FnOnce(Pop<'_>) -> R,
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
    ) -> Option<task::Notified<Arc<Handle>>> {
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
    pub(super) fn drain_into(&self, dst: &mut Vec<task::Notified<Arc<Handle>>>) {
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

pub(super) struct Pop<'a> {
    entries: &'a mut VecDeque<task::Notified<Arc<Handle>>>,
    remaining: usize,
}

impl Iterator for Pop<'_> {
    type Item = task::Notified<Arc<Handle>>;

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

impl ExactSizeIterator for Pop<'_> {}

impl Drop for Pop<'_> {
    fn drop(&mut self) {
        // Keep the queue's accounting exact even if the consumer intentionally
        // stops early.
        while self.next().is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::State;
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
}
