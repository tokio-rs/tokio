//! A sharded concurrent queue for the blocking pool.
//!
//! This implementation distributes tasks across multiple shards to reduce
//! lock contention when many threads are spawning blocking tasks concurrently.
//! The push operations use per-shard locking, while notifications use a global
//! condvar for simplicity.
//!
//! The queue adapts to the current concurrency level by using fewer shards
//! when there are few threads, which improves cache locality and reduces
//! lock contention on the active shards.

use crate::loom::sync::{Condvar, Mutex};

use std::collections::VecDeque;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Duration;

use super::pool::Task;

/// Number of shards. Must be a power of 2.
const NUM_SHARDS: usize = 16;

/// A single shard containing a queue protected by its own mutex.
struct Shard {
    /// The task queue for this shard.
    queue: Mutex<VecDeque<Task>>,
    /// Number of tasks in this shard's queue. Used for fast empty checks.
    len: AtomicUsize,
}

impl Shard {
    fn new() -> Self {
        Shard {
            queue: Mutex::new(VecDeque::new()),
            len: AtomicUsize::new(0),
        }
    }

    /// Push a task to this shard's queue.
    fn push(&self, task: Task) {
        let mut queue = self.queue.lock();
        queue.push_back(task);
        // Store with Release to ensure the task is visible before len is updated
        self.len.store(queue.len(), Release);
    }

    /// Try to pop a task from this shard's queue.
    fn pop(&self) -> Option<Task> {
        // Fast path: check if empty without locking
        if self.len.load(Acquire) == 0 {
            return None;
        }

        let mut queue = self.queue.lock();
        let task = queue.pop_front();
        if task.is_some() {
            self.len.store(queue.len(), Release);
        }
        task
    }
}

/// A sharded queue that distributes tasks across multiple shards.
pub(super) struct ShardedQueue {
    /// The shards - each with its own mutex-protected queue.
    shards: [Shard; NUM_SHARDS],
    /// Atomic counter for round-robin task distribution.
    push_index: AtomicUsize,
    /// Tracks the highest shard index that has ever been pushed to.
    /// This allows `pop()` to skip checking shards that have never had tasks,
    /// which is important for maintaining low overhead at low concurrency.
    /// Only increases, never decreases (even when shards become empty).
    max_shard_pushed: AtomicUsize,
    /// Global shutdown flag.
    shutdown: AtomicBool,
    /// Global condition variable for worker notifications.
    /// We use a single condvar to avoid the complexity of per-shard waiting.
    condvar: Condvar,
    /// Mutex to pair with the condvar. Only held during wait, not during push/pop.
    condvar_mutex: Mutex<()>,
}

/// Calculate the effective number of shards to use based on thread count.
/// Uses fewer shards at low concurrency for better cache locality.
#[inline]
fn effective_shards(num_threads: usize) -> usize {
    match num_threads {
        0..=2 => 2,
        3..=4 => 4,
        5..=8 => 8,
        _ => NUM_SHARDS,
    }
}

impl ShardedQueue {
    pub(super) fn new() -> Self {
        ShardedQueue {
            shards: std::array::from_fn(|_| Shard::new()),
            push_index: AtomicUsize::new(0),
            max_shard_pushed: AtomicUsize::new(0),
            shutdown: AtomicBool::new(false),
            condvar: Condvar::new(),
            condvar_mutex: Mutex::new(()),
        }
    }

    /// Push a task to the queue.
    ///
    /// `num_threads` is a hint about the current thread count, used to
    /// adaptively choose how many shards to distribute across.
    pub(super) fn push(&self, task: Task, num_threads: usize) {
        let num_shards = effective_shards(num_threads);
        let mask = num_shards - 1;
        let index = self.push_index.fetch_add(1, Relaxed) & mask;

        // Update max_shard_pushed BEFORE pushing the task.
        self.max_shard_pushed.fetch_max(index, Release);

        self.shards[index].push(task);
    }

    /// Notify one waiting worker that a task is available.
    pub(super) fn notify_one(&self) {
        self.condvar.notify_one();
    }

    /// Notify all waiting workers (used during shutdown).
    pub(super) fn notify_all(&self) {
        self.condvar.notify_all();
    }

    /// Try to pop a task, checking the preferred shard first, then others.
    ///
    /// Only checks shards up to `max_shard_pushed` since tasks can only exist
    /// in shards that have been pushed to.
    pub(super) fn pop(&self, preferred_shard: usize) -> Option<Task> {
        // Only check shards that have ever had tasks pushed to them
        let max_shard = self.max_shard_pushed.load(Acquire);
        let num_shards_to_check = max_shard + 1;

        // Check shards starting from preferred, wrapping within active range
        let start = preferred_shard % num_shards_to_check;
        for i in 0..num_shards_to_check {
            let index = (start + i) % num_shards_to_check;
            if let Some(task) = self.shards[index].pop() {
                return Some(task);
            }
        }

        None
    }

    /// Drain all tasks from the queue, calling the provided closure on each.
    pub(super) fn drain<F>(&self, mut f: F)
    where
        F: FnMut(Task),
    {
        for shard in &self.shards {
            loop {
                let mut queue = shard.queue.lock();
                if let Some(task) = queue.pop_front() {
                    shard.len.store(queue.len(), Release);
                    drop(queue);
                    f(task);
                } else {
                    break;
                }
            }
        }
    }

    /// Set the shutdown flag and wake all workers.
    pub(super) fn shutdown(&self) {
        self.shutdown.store(true, Release);
        self.notify_all();
    }

    /// Check if shutdown has been initiated.
    pub(super) fn is_shutdown(&self) -> bool {
        self.shutdown.load(Acquire)
    }

    /// Wait for a task with timeout.
    pub(super) fn wait_for_task(&self, preferred_shard: usize, timeout: Duration) -> WaitResult {
        if self.is_shutdown() {
            return WaitResult::Shutdown;
        }

        // Try to pop without waiting first
        if let Some(task) = self.pop(preferred_shard) {
            return WaitResult::Task(task);
        }

        // Acquire the condvar mutex before waiting
        let guard = self.condvar_mutex.lock();

        // Double-check shutdown and tasks after acquiring lock, as state may
        // have changed while we were waiting for the lock
        if self.is_shutdown() {
            return WaitResult::Shutdown;
        }
        if let Some(task) = self.pop(preferred_shard) {
            return WaitResult::Task(task);
        }

        // Wait for notification or timeout
        let result = self.condvar.wait_timeout(guard, timeout).unwrap();
        let timed_out = result.1.timed_out();

        // Drop the lock before doing further work
        drop(result.0);

        if self.is_shutdown() {
            return WaitResult::Shutdown;
        }

        // Try to get a task
        if let Some(task) = self.pop(preferred_shard) {
            return WaitResult::Task(task);
        }

        if timed_out {
            WaitResult::Timeout
        } else {
            WaitResult::Spurious
        }
    }
}

/// Result of waiting for a task.
pub(super) enum WaitResult {
    /// A task was found.
    Task(Task),
    /// The wait timed out.
    Timeout,
    /// Shutdown was initiated.
    Shutdown,
    /// Spurious wakeup, no task found.
    Spurious,
}
