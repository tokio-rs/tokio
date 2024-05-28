use crate::runtime::metrics::Histogram;
use crate::runtime::Config;
use crate::util::metric_atomics::MetricAtomicU64;
// This is NOT the Loom atomic. To avoid an unnecessary state explosion in loom,
// all metrics use regular atomics.
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;

/// Retrieve runtime worker metrics.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// [unstable]: crate#unstable-features
#[derive(Debug)]
#[repr(align(128))]
pub(crate) struct WorkerMetrics {
    ///  Number of times the worker parked.
    pub(crate) park_count: MetricAtomicU64,

    /// Number of times the worker woke then parked again without doing work.
    pub(crate) noop_count: MetricAtomicU64,

    /// Number of tasks the worker stole.
    pub(crate) steal_count: MetricAtomicU64,

    /// Number of times the worker stole
    pub(crate) steal_operations: MetricAtomicU64,

    /// Number of tasks the worker polled.
    pub(crate) poll_count: MetricAtomicU64,

    /// EWMA task poll time, in nanoseconds.
    pub(crate) mean_poll_time: MetricAtomicU64,

    /// Amount of time the worker spent doing work vs. parking.
    pub(crate) busy_duration_total: MetricAtomicU64,

    /// Number of tasks scheduled for execution on the worker's local queue.
    pub(crate) local_schedule_count: MetricAtomicU64,

    /// Number of tasks moved from the local queue to the global queue to free space.
    pub(crate) overflow_count: MetricAtomicU64,

    /// Number of tasks currently in the local queue. Used only by the
    /// current-thread scheduler.
    pub(crate) queue_depth: AtomicUsize,

    /// If `Some`, tracks the number of polls by duration range.
    pub(super) poll_count_histogram: Option<Histogram>,
}

impl WorkerMetrics {
    pub(crate) fn from_config(config: &Config) -> WorkerMetrics {
        let mut worker_metrics = WorkerMetrics::new();
        worker_metrics.poll_count_histogram = config
            .metrics_poll_count_histogram
            .as_ref()
            .map(|histogram_builder| histogram_builder.build());
        worker_metrics
    }

    pub(crate) fn new() -> WorkerMetrics {
        WorkerMetrics {
            park_count: MetricAtomicU64::new(0),
            noop_count: MetricAtomicU64::new(0),
            steal_count: MetricAtomicU64::new(0),
            steal_operations: MetricAtomicU64::new(0),
            poll_count: MetricAtomicU64::new(0),
            mean_poll_time: MetricAtomicU64::new(0),
            overflow_count: MetricAtomicU64::new(0),
            busy_duration_total: MetricAtomicU64::new(0),
            local_schedule_count: MetricAtomicU64::new(0),
            queue_depth: AtomicUsize::new(0),
            poll_count_histogram: None,
        }
    }

    pub(crate) fn queue_depth(&self) -> usize {
        self.queue_depth.load(Relaxed)
    }

    pub(crate) fn set_queue_depth(&self, len: usize) {
        self.queue_depth.store(len, Relaxed);
    }
}
