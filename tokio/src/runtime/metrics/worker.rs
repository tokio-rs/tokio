use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

/// Retreive runtime worker metrics.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// [unstable]: crate#unstable-features
#[derive(Debug)]
#[repr(align(128))]
pub struct WorkerMetrics {
    ///  Number of times the worker parked.
    pub(super) park_count: AtomicU64,

    /// Number of times the worker woke then parked again without doing work.
    pub(super) noop_count: AtomicU64,

    /// Number of times the worker attempted to steal.
    pub(super) steal_count: AtomicU64,

    /// Number of tasks the worker polled.
    pub(super) poll_count: AtomicU64,

    /// Number of tasks stolen from the current worker.
    pub(super) stolen_count: AtomicU64,

    /// Amount of time the worker spent doing work vs. parking.
    pub(super) busy_duration_total: AtomicU64,

    /// Number of tasks scheduled for execution on the worker's local queue.
    pub(super) local_schedule_count: AtomicU64,

    /// Number of tasks moved from the local queue to the global queue to free space.
    pub(super) overflow_count: AtomicU64,
}

impl WorkerMetrics {
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