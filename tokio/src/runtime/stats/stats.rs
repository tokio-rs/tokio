//! This file contains the types necessary to collect various types of stats.
use crate::loom::sync::atomic::{AtomicU64, Ordering::Relaxed};

use std::convert::TryFrom;
use std::time::{Duration, Instant};

/// This type contains methods to retrieve stats from a Tokio runtime.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// [unstable]: crate#unstable-features
#[derive(Debug)]
pub struct RuntimeStats {
    /// Number of tasks that are scheduled from outside the runtime.
    remote_schedule_count: AtomicU64,
    /// Tracks per-worker performance counters
    workers: Box<[WorkerStats]>,
}

/// This type contains methods to retrieve stats from a worker thread on a Tokio runtime.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// [unstable]: crate#unstable-features
#[derive(Debug)]
#[repr(align(128))]
pub struct WorkerStats {
    ///  Number of times the worker parked.
    park_count: AtomicU64,

    /// Number of times the worker woke then parked again without doing work.
    noop_count: AtomicU64,

    /// Number of times the worker attempted to steal.
    steal_count: AtomicU64,

    /// Number of tasks the worker polled.
    poll_count: AtomicU64,

    /// Number of tasks stolen from the current worker.
    stolen_count: AtomicU64,

    /// Amount of time the worker spent doing work vs. parking.
    busy_duration_total: AtomicU64,

    /// Number of tasks scheduled for execution on the worker's local queue.
    local_schedule_count: AtomicU64,

    /// Number of tasks moved from the local queue to the global queue to free space.
    overflow_count: AtomicU64,
}

impl RuntimeStats {
    pub(crate) fn new(worker_threads: usize) -> Self {
        let mut workers = Vec::with_capacity(worker_threads);
        for _ in 0..worker_threads {
            workers.push(WorkerStats {
                park_count: AtomicU64::new(0),
                noop_count: AtomicU64::new(0),
                steal_count: AtomicU64::new(0),
                poll_count: AtomicU64::new(0),
                stolen_count: AtomicU64::new(0),
                overflow_count: AtomicU64::new(0),
                busy_duration_total: AtomicU64::new(0),
                local_schedule_count: AtomicU64::new(0),
            });
        }

        Self {
            remote_schedule_count: AtomicU64::new(0),
            workers: workers.into_boxed_slice(),
        }
    }

    /// Returns the number of tasks scheduled from **outside** of the runtime.
    ///
    /// Tasks scheduled from outside of the runtime go via the runtime's
    /// injection queue, which is usually is slower.
    pub fn remote_schedule_count(&self) -> u64 {
        self.remote_schedule_count.load(Relaxed)
    }

    /// Returns a slice containing the worker stats for each worker thread.
    pub fn workers(&self) -> impl Iterator<Item = &WorkerStats> {
        self.workers.iter()
    }

    /// Increment the number of tasks scheduled externally
    pub(crate) fn inc_remote_schedule_count(&self) {
        self.remote_schedule_count.fetch_add(1, Relaxed);
    }

    pub(crate) fn worker(&self, index: usize) -> &WorkerStats {
        &self.workers[index]
    }
}

impl WorkerStats {
    /// Returns the total number of times this worker thread has parked.
    pub fn park_count(&self) -> u64 {
        self.park_count.load(Relaxed)
    }

    /// Returns the number of times this worker unparked but performed no work.
    ///
    /// This is the false-positive wake count.
    pub fn noop_count(&self) -> u64 {
        self.noop_count.load(Relaxed)
    }

    /// Returns the number of tasks this worker has stolen from other worker
    /// threads.
    pub fn steal_count(&self) -> u64 {
        self.steal_count.load(Relaxed)
    }

    /// Returns the number of tasks that were stolen from this worker.
    pub fn stolen_count(&self) -> u64 {
        self.stolen_count.load(Relaxed)
    }

    /// Returns the number of times this worker has polled a task.
    pub fn poll_count(&self) -> u64 {
        self.poll_count.load(Relaxed)
    }

    /// Returns the total amount of time this worker has been busy for.
    pub fn total_busy_duration(&self) -> Duration {
        Duration::from_nanos(self.busy_duration_total.load(Relaxed))
    }

    /// TODO
    pub fn local_schedule_count(&self) -> u64 {
        self.local_schedule_count.load(Relaxed)
    }

    /// Returns the number of tasks moved from this worker's local queue to the
    /// remote queue.
    pub fn overflow_count(&self) -> u64 {
        self.overflow_count.load(Relaxed)
    }

    pub(crate) fn incr_stolen_count(&self, n: u16) {
        self.stolen_count.fetch_add(n as _, Relaxed);
    }
}

pub(crate) struct WorkerStatsBatcher {
    /// Identifies the worker within the runtime.
    my_index: usize,

    /// Number of times the worker parked
    park_count: u64,

    /// Number of times the worker woke w/o doing work.
    noop_count: u64,

    /// Number of times stolen
    steal_count: u64,

    /// Number of tasks that were polled by the worker.
    poll_count: u64,

    /// Number of tasks polled when the worker entered park. This is used to
    /// track the noop count.
    poll_count_on_last_park: u64,

    /// Number of tasks that were scheduled locally on this worker.
    local_schedule_count: u64,

    /// Number of tasks moved to the global queue to make space in the local
    /// queue
    overflow_count: u64,

    /// The total busy duration in nanoseconds.
    busy_duration_total: u64,
    last_resume_time: Instant,
}

impl WorkerStatsBatcher {
    pub(crate) fn new(my_index: usize) -> Self {
        Self {
            my_index,
            park_count: 0,
            noop_count: 0,
            steal_count: 0,
            poll_count: 0,
            poll_count_on_last_park: 0,
            local_schedule_count: 0,
            overflow_count: 0,
            busy_duration_total: 0,
            last_resume_time: Instant::now(),
        }
    }
    pub(crate) fn submit(&mut self, to: &RuntimeStats) {
        let worker = &to.workers[self.my_index];

        worker.park_count.store(self.park_count, Relaxed);
        worker.noop_count.store(self.noop_count, Relaxed);
        worker.steal_count.store(self.steal_count, Relaxed);
        worker.poll_count.store(self.poll_count, Relaxed);

        worker
            .busy_duration_total
            .store(self.busy_duration_total, Relaxed);

        worker
            .local_schedule_count
            .store(self.local_schedule_count, Relaxed);
        worker.overflow_count.store(self.overflow_count, Relaxed);
    }

    /// The worker is about to park.
    pub(crate) fn about_to_park(&mut self) {
        self.park_count += 1;

        if self.poll_count_on_last_park == self.poll_count {
            self.noop_count += 1;
        } else {
            self.poll_count_on_last_park = self.poll_count;
        }

        let busy_duration = self.last_resume_time.elapsed();
        let busy_duration = u64::try_from(busy_duration.as_nanos()).unwrap_or(u64::MAX);
        self.busy_duration_total += busy_duration;
    }

    pub(crate) fn returned_from_park(&mut self) {
        self.last_resume_time = Instant::now();
    }

    pub(crate) fn inc_local_schedule_count(&mut self) {
        self.local_schedule_count += 1;
    }

    pub(crate) fn incr_poll_count(&mut self) {
        self.poll_count += 1;
    }
}

cfg_rt_multi_thread! {
    impl WorkerStatsBatcher {
        pub(crate) fn incr_steal_count(&mut self, by: u16) {
            self.steal_count += by as u64;
        }

        pub(crate) fn incr_overflow_count(&mut self, by: u16) {
            self.overflow_count += by as u64;
        }
    }
}
