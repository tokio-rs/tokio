//! A sharded concurrent queue for the blocking pool.
//!
//! This implementation distributes tasks across multiple shards to reduce
//! lock contention when many threads are spawning blocking tasks concurrently.
//! The push operations use per-shard locking, while notifications use a global
//! condvar for simplicity.
//!
//! For shard selection, we use the same approach as `sync::watch`: prefer
//! randomness when available to reduce contention, falling back to circular
//! access when the random number generator is not available.

use crate::loom::sync::{Condvar, Mutex};

use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
#[cfg(loom)]
use std::sync::atomic::AtomicUsize;
#[cfg(loom)]
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::time::Duration;

use super::pool::Task;

/// Number of shards. Must be a power of 2.
/// Under loom, use a single shard to keep the state space tractable —
/// the concurrency properties we need to verify (condvar signaling,
/// shutdown ordering) are independent of shard count.
#[cfg(not(loom))]
const NUM_SHARDS: usize = 16;
#[cfg(loom)]
const NUM_SHARDS: usize = 1;

/// A single shard containing a queue protected by its own mutex.
struct Shard {
    /// The task queue for this shard.
    queue: Mutex<VecDeque<Task>>,
}

impl Shard {
    fn new() -> Self {
        Shard {
            queue: Mutex::new(VecDeque::new()),
        }
    }

    /// Push a task to this shard's queue.
    fn push(&self, task: Task) {
        let mut queue = self.queue.lock();

        // Check if pushing would require reallocation (when len == capacity).
        // If so, allocate outside the lock to avoid blocking readers.
        while queue.len() == queue.capacity() {
            let current_len = queue.len();
            // Use 2x growth factor, minimum 4
            let new_cap = current_len.saturating_mul(2).max(4);

            // Release lock before allocating
            drop(queue);

            let mut new_queue = VecDeque::with_capacity(new_cap);

            queue = self.queue.lock();
            // If the queue is:
            // a) Not full anymore => push to the current queue
            // b) Full and our new queue is big enough => copy items to the new
            //    queue and push to it.
            // c) Full and our new queue is too small => try again.
            if queue.len() == queue.capacity() {
                if new_queue.capacity() > queue.len() {
                    new_queue.extend(queue.drain(..));
                    *queue = new_queue;
                    break;
                }
            } else {
                break;
            }
        }

        queue.push_back(task);
    }

    /// Try to pop a task from this shard's queue.
    fn pop(&self) -> Option<Task> {
        let mut queue = self.queue.lock();
        queue.pop_front()
    }
}

/// A sharded queue that distributes tasks across multiple shards.
pub(super) struct ShardedQueue {
    /// The shards - each with its own mutex-protected queue.
    shards: [Shard; NUM_SHARDS],
    /// Atomic counter for round-robin task distribution.
    /// Only used when randomness is not available (loom).
    #[cfg(loom)]
    push_index: AtomicUsize,
    /// Global shutdown flag.
    shutdown: AtomicBool,
    /// Global condition variable for worker notifications.
    /// We use a single condvar to avoid the complexity of per-shard waiting.
    condvar: Condvar,
    /// Mutex paired with the condvar. Protects the notification counter
    /// (`num_notify`), which tracks how many tasks have been pushed and
    /// need to be picked up by idle workers.
    condvar_mutex: Mutex<u32>,
}

impl ShardedQueue {
    pub(super) fn new() -> Self {
        ShardedQueue {
            shards: std::array::from_fn(|_| Shard::new()),
            #[cfg(loom)]
            push_index: AtomicUsize::new(0),
            shutdown: AtomicBool::new(false),
            condvar: Condvar::new(),
            condvar_mutex: Mutex::new(0),
        }
    }

    /// Select the next shard index for pushing a task -- when the RNG is
    /// available.
    #[cfg(not(loom))]
    fn next_push_index(&self, num_shards: usize) -> usize {
        crate::runtime::context::thread_rng_n(num_shards as u32) as usize
    }

    /// Select the next shard index for pushing a task -- when the RNG is not
    /// available (loom).
    #[cfg(loom)]
    fn next_push_index(&self, num_shards: usize) -> usize {
        self.push_index.fetch_add(1, Relaxed) & (num_shards - 1)
    }

    /// Push a task to the queue.
    pub(super) fn push(&self, task: Task) {
        let index = self.next_push_index(NUM_SHARDS);
        self.shards[index].push(task);
    }

    /// Notify one waiting worker that a task is available.
    ///
    /// Increments the notification counter under `condvar_mutex` and signals
    /// the condvar. The counter acts as a persistent notification that cannot
    /// be lost — even if no worker is currently waiting on the condvar, the
    /// next worker to enter `wait_for_task` will see the counter and know
    /// there is work to do.
    pub(super) fn notify_one(&self) {
        let mut guard = self.condvar_mutex.lock();
        *guard += 1;
        drop(guard);
        self.condvar.notify_one();
    }

    /// Try to pop a task, checking the preferred shard first, then others.
    pub(super) fn pop(&self, preferred_shard: usize) -> Option<Task> {
        // Check shards starting from preferred, wrapping around
        let start = preferred_shard % NUM_SHARDS;
        for i in 0..NUM_SHARDS {
            let index = (start + i) % NUM_SHARDS;
            if let Some(task) = self.shards[index].pop() {
                return Some(task);
            }
        }

        None
    }

    /// Set the shutdown flag and wake all workers.
    pub(super) fn shutdown(&self) {
        // Set the flag while holding condvar_mutex so that any worker
        // currently inside `wait_for_task` (which also holds condvar_mutex
        // while checking) is guaranteed to see the flag on its next check.
        {
            let _guard = self.condvar_mutex.lock();
            self.shutdown.store(true, Release);
        }
        self.condvar.notify_all();
    }

    /// Check if shutdown has been initiated.
    pub(super) fn is_shutdown(&self) -> bool {
        self.shutdown.load(Acquire)
    }

    /// Wait for a task notification with timeout. Returns when a task has
    /// been pushed (the caller should then `pop`), shutdown occurs, or the
    /// wait times out.
    ///
    /// Uses a notification counter under `condvar_mutex` to prevent lost
    /// wakeups — the same pattern as the original single-mutex blocking pool.
    pub(super) fn wait_for_task(&self, preferred_shard: usize, timeout: Duration) -> WaitResult {
        let mut guard = self.condvar_mutex.lock();

        loop {
            if self.is_shutdown() {
                return WaitResult::Shutdown;
            }

            if *guard > 0 {
                // A notification is pending — a task was pushed.
                *guard -= 1;
                drop(guard);
                // Pop outside the condvar_mutex to avoid holding two locks.
                if let Some(task) = self.pop(preferred_shard) {
                    return WaitResult::Task(task);
                }
                // The task was already consumed in the caller's BUSY loop
                // (race between push+notify and the worker's pop loop).
                // Re-enter the wait.
                guard = self.condvar_mutex.lock();
                continue;
            }

            let (g, timeout_result) = self.condvar.wait_timeout(guard, timeout).unwrap();
            guard = g;

            if timeout_result.timed_out() && *guard == 0 {
                // Double-check: shutdown may have raced with the timeout.
                if self.is_shutdown() {
                    return WaitResult::Shutdown;
                }
                return WaitResult::Timeout;
            }
            // Woken by notify or spurious wakeup — loop back to check.
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
}
