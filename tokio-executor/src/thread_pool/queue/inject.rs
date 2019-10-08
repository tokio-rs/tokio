use crate::loom::sync::Arc;
use crate::task::Task;
use crate::thread_pool::queue::Cluster;

pub(crate) struct Inject<T: 'static> {
    cluster: Arc<Cluster<T>>,
}

impl<T: 'static> Inject<T> {
    pub(super) fn new(cluster: Arc<Cluster<T>>) -> Inject<T> {
        Inject { cluster }
    }

    /// Push a value onto the queue
    pub(crate) fn push(&self, task: Task<T>) {
        self.cluster.global.push(task);
    }

    /// Close the queue
    ///
    /// Returns `true` if the channel was closed. `false` indicates the pool was
    /// previously closed.
    pub(crate) fn close(&self) -> bool {
        self.cluster.global.close()
    }
}
