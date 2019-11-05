use crate::executor::task::Task;
use crate::executor::thread_pool::queue::{local, Cluster, Inject};
use crate::loom::sync::Arc;

use std::cell::Cell;
use std::fmt;

pub(crate) struct Worker<T: 'static> {
    cluster: Arc<Cluster<T>>,
    index: u16,
    /// Task to pop next
    next: Cell<Option<Task<T>>>,
}

impl<T: 'static> Worker<T> {
    pub(super) fn new(cluster: Arc<Cluster<T>>, index: usize) -> Worker<T> {
        Worker {
            cluster,
            index: index as u16,
            next: Cell::new(None),
        }
    }

    pub(crate) fn injector(&self) -> Inject<T> {
        Inject::new(self.cluster.clone())
    }

    /// Returns `true` if the queue is closed
    pub(crate) fn is_closed(&self) -> bool {
        self.cluster.global.is_closed()
    }

    /// Push to the local queue.
    ///
    /// If the local queue is full, the task is pushed onto the global queue.
    ///
    /// # Return
    ///
    /// Returns `true` if the pushed task can be stolen by another worker.
    pub(crate) fn push(&self, task: Task<T>) -> bool {
        let prev = self.next.take();
        let ret = prev.is_some();

        if let Some(prev) = prev {
            // safety: we guarantee that only one thread pushes to this local
            // queue at a time.
            unsafe {
                self.local().push(prev, &self.cluster.global);
            }
        }

        self.next.set(Some(task));

        ret
    }

    pub(crate) fn push_yield(&self, task: Task<T>) {
        unsafe { self.local().push(task, &self.cluster.global) }
    }

    /// Pop a task checking the local queue first.
    pub(crate) fn pop_local_first(&self) -> Option<Task<T>> {
        self.local_pop().or_else(|| self.cluster.global.pop())
    }

    /// Pop a task checking the global queue first.
    pub(crate) fn pop_global_first(&self) -> Option<Task<T>> {
        self.cluster.global.pop().or_else(|| self.local_pop())
    }

    /// Steal from other local queues.
    ///
    /// `start` specifies the queue from which to start stealing.
    pub(crate) fn steal(&self, start: usize) -> Option<Task<T>> {
        let num_queues = self.cluster.local.len();

        for i in 0..num_queues {
            let i = (start + i) % num_queues;

            if i == self.index as usize {
                continue;
            }

            // safety: we own the dst queue
            let ret = unsafe { self.cluster.local[i].steal(self.local()) };

            if ret.is_some() {
                return ret;
            }
        }

        None
    }

    /// An approximation of whether or not the queue is empty.
    pub(crate) fn is_empty(&self) -> bool {
        for local_queue in &self.cluster.local[..] {
            if !local_queue.is_empty() {
                return false;
            }
        }

        self.cluster.global.is_empty()
    }

    fn local_pop(&self) -> Option<Task<T>> {
        if let Some(task) = self.next.take() {
            return Some(task);
        }
        // safety: we guarantee that only one thread pushes to this local queue
        // at a time.
        unsafe { self.local().pop() }
    }

    fn local(&self) -> &local::Queue<T> {
        &self.cluster.local[self.index as usize]
    }
}

impl<T: 'static> fmt::Debug for Worker<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("queue::Worker")
            .field("cluster", &"...")
            .field("index", &self.index)
            .finish()
    }
}
