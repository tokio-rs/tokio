use task::Task;

use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};

#[derive(Debug)]
pub(crate) struct Queue {
    // TODO(stjepang): Use a custom, faster MPMC queue implementation that supports `steal_many()`.
    chan: (Sender<Arc<Task>>, Receiver<Arc<Task>>),
}

// ===== impl Queue =====

impl Queue {
    /// Create a new, empty, `Queue`.
    pub fn new() -> Queue {
        Queue {
            chan: unbounded(),
        }
    }

    /// Push a task onto the queue.
    #[inline]
    pub fn push(&self, task: Arc<Task>) {
        self.chan.0.send(task).unwrap();
    }

    /// Pop a task from the queue.
    #[inline]
    pub fn pop(&self) -> Option<Arc<Task>> {
        self.chan.1.try_recv().ok()
    }
}
