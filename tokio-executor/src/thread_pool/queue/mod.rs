//! The threadpool's task queue system.

mod global;
mod inject;
mod local;
mod worker;

pub(crate) use self::inject::Inject;
pub(crate) use self::worker::Worker;

use crate::loom::sync::Arc;

pub(crate) fn build<T: 'static>(workers: usize) -> Vec<Worker<T>> {
    let local: Vec<_> = (0..workers).map(|_| local::Queue::new()).collect();

    let cluster = Arc::new(Cluster {
        local: local.into_boxed_slice(),
        global: global::Queue::new(),
    });

    (0..workers)
        .map(|index| Worker::new(cluster.clone(), index))
        .collect()
}

struct Cluster<T: 'static> {
    /// per-worker local queues
    local: Box<[local::Queue<T>]>,
    global: global::Queue<T>,
}

impl<T: 'static> Drop for Cluster<T> {
    fn drop(&mut self) {
        // Drain all the queues
        for queue in &self.local[..] {
            while let Some(_) = unsafe { queue.pop() } {}
        }

        while let Some(_) = self.global.pop() {}
    }
}
