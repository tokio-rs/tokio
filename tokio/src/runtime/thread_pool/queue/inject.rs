use crate::loom::sync::Arc;
use crate::runtime::task::Task;
use crate::runtime::thread_pool::queue::Cluster;

pub(crate) struct Inject<T: 'static> {
    cluster: Arc<Cluster<T>>,
}

impl<T: 'static> Inject<T> {
    pub(super) fn new(cluster: Arc<Cluster<T>>) -> Inject<T> {
        Inject { cluster }
    }

    /// Push a value onto the queue
    pub(crate) fn push<F>(&self, task: Task<T>, f: F)
    where
        F: FnOnce(Result<(), Task<T>>),
    {
        self.cluster.global.push(task, f)
    }

    /// Check if the queue has been closed
    pub(crate) fn is_closed(&self) -> bool {
        self.cluster.global.is_closed()
    }

    /// Close the queue
    ///
    /// Returns `true` if the channel was closed. `false` indicates the pool was
    /// previously closed.
    pub(crate) fn close(&self) -> bool {
        self.cluster.global.close()
    }

    /// Wait for all locks on the queue to drop.
    ///
    /// This is done by locking w/o doing anything.
    pub(crate) fn wait_for_unlocked(&self) {
        self.cluster.global.wait_for_unlocked();
    }
}
