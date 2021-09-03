//! This file contains the types necessary to collect various types of stats.
use crate::loom::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// This type contains methods to retrieve stats from a Tokio runtime.
#[derive(Debug)]
pub struct RuntimeStats {
    workers: Box<[WorkerStats]>,
}

/// This type contains methods to retrieve stats from a worker thread on a Tokio runtime.
#[derive(Debug)]
#[repr(align(128))]
pub struct WorkerStats {
    park_count: AtomicU64,
    steal_count: AtomicU64,
    poll_count: AtomicU64,
}

impl RuntimeStats {
    pub(crate) fn new(worker_threads: usize) -> Self {
        let mut workers = Vec::with_capacity(worker_threads);
        for _ in 0..worker_threads {
            workers.push(WorkerStats {
                park_count: AtomicU64::new(0),
                steal_count: AtomicU64::new(0),
                poll_count: AtomicU64::new(0),
            });
        }

        Self {
            workers: workers.into_boxed_slice(),
        }
    }

    /// Returns a slice containing the worker stats for each worker thread.
    pub fn workers(&self) -> impl Iterator<Item = &WorkerStats> {
        self.workers.iter()
    }
}

impl WorkerStats {
    /// Returns the total number of times this worker thread has parked.
    pub fn park_count(&self) -> u64 {
        self.park_count.load(Relaxed)
    }

    /// Returns the number of tasks this worker has stolen from other worker
    /// threads.
    pub fn steal_count(&self) -> u64 {
        self.steal_count.load(Relaxed)
    }

    /// Returns the number of times this worker has polled a task.
    pub fn poll_count(&self) -> u64 {
        self.poll_count.load(Relaxed)
    }
}

pub(crate) struct WorkerStatsBatcher {
    my_index: usize,
    park_count: u64,
    steal_count: u64,
    poll_count: u64,
}

impl WorkerStatsBatcher {
    pub(crate) fn new(my_index: usize) -> Self {
        Self {
            my_index,
            park_count: 0,
            steal_count: 0,
            poll_count: 0,
        }
    }
    pub(crate) fn submit(&mut self, to: &RuntimeStats) {
        let worker = &to.workers[self.my_index];

        worker.park_count.store(self.park_count, Relaxed);
        worker.steal_count.store(self.steal_count, Relaxed);
        worker.poll_count.store(self.poll_count, Relaxed);
    }

    pub(crate) fn about_to_park(&mut self) {
        self.park_count += 1;
    }

    pub(crate) fn returned_from_park(&mut self) {}

    #[cfg(feature = "rt-multi-thread")]
    pub(crate) fn incr_steal_count(&mut self, by: u16) {
        self.steal_count += u64::from(by);
    }

    pub(crate) fn incr_poll_count(&mut self) {
        self.poll_count += 1;
    }
}
