//! This file contains the types necessary to collect various types of metrics.
use crate::loom::sync::atomic::{AtomicU64, Ordering};

pub(crate) struct RuntimeMetrics {
    workers: Box<[WorkerMetrics]>,
}

pub(crate) struct WorkerMetrics {
    park_count: AtomicU64,
    steal_count: AtomicU64,
    poll_count: AtomicU64,
}


impl RuntimeMetrics {
    pub(crate) fn new(worker_threads: usize) -> Self {
        let mut workers = Vec::with_capacity(worker_threads);
        for _ in 0..worker_threads {
            workers.push(WorkerMetrics {
                park_count: AtomicU64::new(0),
                steal_count: AtomicU64::new(0),
                poll_count: AtomicU64::new(0),
            });
        }

        Self {
            workers: workers.into_boxed_slice(),
        }
    }
}


pub(crate) struct WorkerMetricsBatcher {
    my_index: usize,
    park_count: u64,
    steal_count: u64,
    poll_count: u64,
}

impl WorkerMetricsBatcher {
    pub(crate) fn new(my_index: usize) -> Self {
        Self {
            my_index,
            park_count: 0,
            steal_count: 0,
            poll_count: 0,
        }
    }
    pub(crate) fn submit(&mut self, to: &RuntimeMetrics) {
        let worker = &to.workers[self.my_index];

        worker.park_count.store(self.park_count, Ordering::Relaxed);
        worker.steal_count.store(self.steal_count, Ordering::Relaxed);
        worker.poll_count.store(self.poll_count, Ordering::Relaxed);
    }

    pub(crate) fn incr_park_count(&mut self) {
        self.park_count += 1;
    }

    pub(crate) fn incr_steal_count(&mut self, by: u16) {
        self.steal_count += u64::from(by);
    }

    pub(crate) fn incr_poll_count(&mut self) {
        self.poll_count += 1;
    }
}
