//! This file contains the types necessary to collect various types of metrics.
use crate::loom::sync::atomic::{AtomicU64, Ordering};
use crate::runtime::metrics::counter_duration::{CounterDuration, AtomicCounterDuration};

use std::time::{Instant, Duration};

/// This type contains methods to retrieve metrics from a Tokio runtime.
#[derive(Debug)]
pub struct RuntimeMetrics {
    workers: Box<[WorkerMetrics]>,
}

/// This type contains methods to retrieve metrics from a worker thread on a Tokio runtime.
#[derive(Debug)]
pub struct WorkerMetrics {
    park_count: AtomicU64,
    steal_count: AtomicU64,
    poll_count: AtomicU64,
    park_to_park: AtomicCounterDuration,
}


impl RuntimeMetrics {
    pub(crate) fn new(worker_threads: usize) -> Self {
        let mut workers = Vec::with_capacity(worker_threads);
        for _ in 0..worker_threads {
            workers.push(WorkerMetrics {
                park_count: AtomicU64::new(0),
                steal_count: AtomicU64::new(0),
                poll_count: AtomicU64::new(0),
                park_to_park: AtomicCounterDuration::default(),
            });
        }

        Self {
            workers: workers.into_boxed_slice(),
        }
    }

    /// Returns a slice containing the worker metrics for each worker thread.
    pub fn workers(&self) -> impl Iterator<Item = &WorkerMetrics> {
        self.workers.iter()
    }
}

impl WorkerMetrics {
    /// Returns the total number of times this worker thread has parked.
    pub fn park_count(&self) -> u64 {
        self.park_count.load(Ordering::Relaxed)
    }

    /// Returns the number of tasks this worker has stolen from other worker
    /// threads.
    pub fn steal_count(&self) -> u64 {
        self.steal_count.load(Ordering::Relaxed)
    }

    /// Returns the number of times this worker has polled a task.
    pub fn poll_count(&self) -> u64 {
        self.poll_count.load(Ordering::Relaxed)
    }

    /// Returns the amount of time the runtime spent working between the last
    /// two times it parked.
    ///
    /// The `u16` is a counter that is incremented by one each time the duration
    /// is changed. The counter will wrap around when it reaches `u16::MAX`.
    pub fn park_to_park(&self) -> (u16, Duration) {
        self.park_to_park.load(Ordering::Relaxed).into_pair()
    }
}

pub(crate) struct WorkerMetricsBatcher {
    my_index: usize,
    park_count: u64,
    steal_count: u64,
    poll_count: u64,
    last_park: Instant,
    park_to_park: CounterDuration,
}

impl WorkerMetricsBatcher {
    pub(crate) fn new(my_index: usize) -> Self {
        Self {
            my_index,
            park_count: 0,
            steal_count: 0,
            poll_count: 0,
            last_park: Instant::now(),
            park_to_park: CounterDuration::default(),
        }
    }
    pub(crate) fn submit(&mut self, to: &RuntimeMetrics) {
        let worker = &to.workers[self.my_index];

        worker.park_count.store(self.park_count, Ordering::Relaxed);
        worker.steal_count.store(self.steal_count, Ordering::Relaxed);
        worker.poll_count.store(self.poll_count, Ordering::Relaxed);
        worker.park_to_park.store(self.park_to_park, Ordering::Relaxed);
    }

    pub(crate) fn incr_park_count(&mut self) {
        self.park_count += 1;
        self.update_park_to_park();
    }

    pub(crate) fn incr_steal_count(&mut self, by: u16) {
        self.steal_count += u64::from(by);
    }

    pub(crate) fn incr_poll_count(&mut self) {
        self.poll_count += 1;
    }

    pub(crate) fn update_park_to_park(&mut self) {
        let now = Instant::now();
        let diff = now - self.last_park;
        self.park_to_park.set_next_duration(diff);
        self.last_park = now;
    }
}
