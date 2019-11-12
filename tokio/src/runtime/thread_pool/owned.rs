use crate::loom::sync::atomic::AtomicUsize;
use crate::runtime::thread_pool::{queue, Shared};
use crate::task::{self, Task};
use crate::util::FastRand;

use std::cell::Cell;

/// Per-worker data accessible only by the thread driving the worker.
#[derive(Debug)]
pub(super) struct Owned<P: 'static> {
    /// Worker generation. This guards concurrent access to the `Owned` struct.
    /// When a worker starts running, it checks that the generation it has
    /// assigned matches the current generation. When it does, the worker has
    /// obtained unique access to the struct. When it fails, another thread has
    /// gained unique access.
    pub(super) generation: AtomicUsize,

    /// Worker tick number. Used to schedule bookkeeping tasks every so often.
    pub(super) tick: Cell<u16>,

    /// Caches the pool run state.
    pub(super) is_running: Cell<bool>,

    /// `true` if the worker is currently searching for more work.
    pub(super) is_searching: Cell<bool>,

    /// `true` when worker notification should be delayed.
    ///
    /// This is used to batch notifications triggered by the parker.
    pub(super) defer_notification: Cell<bool>,

    /// `true` if a task was submitted while `defer_notification` was set
    pub(super) did_submit_task: Cell<bool>,

    /// Fast random number generator
    pub(super) rand: FastRand,

    /// Work queue
    pub(super) work_queue: queue::Worker<Shared<P>>,

    /// List of tasks owned by the worker
    pub(super) owned_tasks: task::OwnedList<Shared<P>>,
}

impl<P> Owned<P>
where
    P: 'static,
{
    pub(super) fn new(work_queue: queue::Worker<Shared<P>>, rand: FastRand) -> Owned<P> {
        Owned {
            generation: AtomicUsize::new(0),
            tick: Cell::new(1),
            is_running: Cell::new(true),
            is_searching: Cell::new(false),
            defer_notification: Cell::new(false),
            did_submit_task: Cell::new(false),
            rand,
            work_queue,
            owned_tasks: task::OwnedList::new(),
        }
    }

    /// Returns `true` if a worker should be notified
    pub(super) fn submit_local(&self, task: Task<Shared<P>>) -> bool {
        let ret = self.work_queue.push(task);

        if self.defer_notification.get() {
            self.did_submit_task.set(true);
            false
        } else {
            ret
        }
    }

    pub(super) fn submit_local_yield(&self, task: Task<Shared<P>>) {
        self.work_queue.push_yield(task);
    }

    pub(super) fn bind_task(&mut self, task: &Task<Shared<P>>) {
        self.owned_tasks.insert(task);
    }

    pub(super) fn release_task(&mut self, task: &Task<Shared<P>>) {
        self.owned_tasks.remove(task);
    }
}
