//! This file contains the types necessary to collect various types of stats.
use crate::loom::sync::atomic::{AtomicU64, Ordering::Relaxed};

use std::convert::TryFrom;
use std::time::{Duration, Instant};

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
    busy_duration_min: AtomicU64,
    busy_duration_max: AtomicU64,
    busy_duration_last: AtomicU64,
    busy_duration_total: AtomicU64,
}

impl RuntimeStats {
    pub(crate) fn new(worker_threads: usize) -> Self {
        let mut workers = Vec::with_capacity(worker_threads);
        for _ in 0..worker_threads {
            workers.push(WorkerStats {
                park_count: AtomicU64::new(0),
                steal_count: AtomicU64::new(0),
                poll_count: AtomicU64::new(0),
                busy_duration_min: AtomicU64::new(0),
                busy_duration_max: AtomicU64::new(0),
                busy_duration_last: AtomicU64::new(0),
                busy_duration_total: AtomicU64::new(0),
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

    /// Returns the duration for which the runtime was busy last time it was busy.
    pub fn busy_duration_last(&self) -> Duration {
        Duration::from_nanos(self.busy_duration_last.load(Relaxed))
    }

    /// Returns the total amount of time this worker has been busy for.
    pub fn busy_duration_total(&self) -> Duration {
        Duration::from_nanos(self.busy_duration_total.load(Relaxed))
    }

    /// Returns the smallest busy duration since the last 16 parks.
    pub fn busy_duration_min(&self) -> Duration {
        Duration::from_nanos(self.busy_duration_min.load(Relaxed))
    }

    /// Returns the largest busy duration since the last 16 parks.
    pub fn busy_duration_max(&self) -> Duration {
        Duration::from_nanos(self.busy_duration_max.load(Relaxed))
    }
}

pub(crate) struct WorkerStatsBatcher {
    my_index: usize,
    park_count: u64,
    steal_count: u64,
    poll_count: u64,
    /// The last 16 busy durations in nanoseconds.
    ///
    /// This array is set to contain the same value 16 times after the first
    /// iteration. Since we only publish the min, max and last value in the
    /// array, then this gives the correct result.
    busy_duration: [u64; 16],
    busy_duration_i: usize,
    /// The total busy duration in nanoseconds.
    busy_duration_total: u64,
    last_resume_time: Instant,
}

impl WorkerStatsBatcher {
    pub(crate) fn new(my_index: usize) -> Self {
        Self {
            my_index,
            park_count: 0,
            steal_count: 0,
            poll_count: 0,
            busy_duration: [0; 16],
            busy_duration_i: usize::MAX,
            busy_duration_total: 0,
            last_resume_time: Instant::now(),
        }
    }
    pub(crate) fn submit(&mut self, to: &RuntimeStats) {
        let worker = &to.workers[self.my_index];

        worker.park_count.store(self.park_count, Relaxed);
        worker.steal_count.store(self.steal_count, Relaxed);
        worker.poll_count.store(self.poll_count, Relaxed);

        let mut min = u64::MAX;
        let mut max = 0;
        let last = self.busy_duration[self.busy_duration_i % 16];
        for &val in &self.busy_duration {
            if val <= min {
                min = val;
            }
            if val >= max {
                max = val;
            }
        }
        worker.busy_duration_min.store(min, Relaxed);
        worker.busy_duration_max.store(max, Relaxed);
        worker.busy_duration_last.store(last, Relaxed);
        worker
            .busy_duration_total
            .store(self.busy_duration_total, Relaxed);
    }

    pub(crate) fn about_to_park(&mut self) {
        self.park_count += 1;

        let busy_duration = self.last_resume_time.elapsed();
        let busy_duration = u64::try_from(busy_duration.as_nanos()).unwrap_or(u64::MAX);
        self.busy_duration_total += busy_duration;
        if self.busy_duration_i == usize::MAX {
            // We are parking for the first time. Set array to contain current
            // duration in every slot.
            self.busy_duration_i = 0;
            self.busy_duration = [busy_duration; 16];
        } else {
            self.busy_duration_i = (self.busy_duration_i + 1) % 16;
            self.busy_duration[self.busy_duration_i] = busy_duration;
        }
    }

    pub(crate) fn returned_from_park(&mut self) {
        self.last_resume_time = Instant::now();
    }

    #[cfg(feature = "rt-multi-thread")]
    pub(crate) fn incr_steal_count(&mut self, by: u16) {
        self.steal_count += u64::from(by);
    }

    pub(crate) fn incr_poll_count(&mut self) {
        self.poll_count += 1;
    }
}
