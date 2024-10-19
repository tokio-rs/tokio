//! This file contains mocks of the types in src/runtime/metrics

use crate::runtime::WorkerMetrics;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, Instant};

pub(crate) struct SchedulerMetrics {}

/// The `MetricsBatch` struct in this mock implementation provides a minimal,
/// simplified version of `batch::MetricsBatch`. It contains only the basic fields
/// required to track the total busy duration (`busy_duration_total`) .
///
/// This mock is used to stabilize the API `worker_total_busy_duration`
/// without relying on the full metrics collection logic. In the real implementation,
/// additional fields provide more detailed tracking of worker activity.
///
/// This mock can be further enriched when stabilizing other worker metrics, such as
/// `worker_thread_id`, `worker_park_count` and so on
///
/// When more worker metrics are stabilized, we can remove this mock and switch back
/// to `batch::MetricsBatch`
pub(crate) struct MetricsBatch {
    /// The total busy duration in nanoseconds.
    busy_duration_total: u64,

    /// Instant at which work last resumed (continued after park).
    processing_scheduled_tasks_started_at: Instant,
}

#[derive(Clone, Default)]
pub(crate) struct HistogramBuilder {}

impl SchedulerMetrics {
    pub(crate) fn new() -> Self {
        Self {}
    }

    /// Increment the number of tasks scheduled externally
    pub(crate) fn inc_remote_schedule_count(&self) {}
}

impl MetricsBatch {
    pub(crate) fn new(_: &WorkerMetrics) -> Self {
        let now = Instant::now();

        MetricsBatch {
            busy_duration_total: 0,
            processing_scheduled_tasks_started_at: now,
        }
    }

    pub(crate) fn submit(&mut self, worker: &WorkerMetrics, _mean_poll_time: u64) {
        worker
            .busy_duration_total
            .store(self.busy_duration_total, Relaxed);
    }
    pub(crate) fn about_to_park(&mut self) {}
    pub(crate) fn unparked(&mut self) {}
    pub(crate) fn inc_local_schedule_count(&mut self) {}
    pub(crate) fn start_processing_scheduled_tasks(&mut self) {
        self.processing_scheduled_tasks_started_at = Instant::now();
    }
    pub(crate) fn end_processing_scheduled_tasks(&mut self) {
        let busy_duration = self.processing_scheduled_tasks_started_at.elapsed();
        self.busy_duration_total += duration_as_u64(busy_duration);
    }
    pub(crate) fn start_poll(&mut self) {}
    pub(crate) fn end_poll(&mut self) {}
}

cfg_rt_multi_thread! {
    impl MetricsBatch {
        pub(crate) fn incr_steal_count(&mut self, _by: u16) {}
        pub(crate) fn incr_steal_operations(&mut self) {}
        pub(crate) fn incr_overflow_count(&mut self) {}
    }
}

fn duration_as_u64(dur: Duration) -> u64 {
    u64::try_from(dur.as_nanos()).unwrap_or(u64::MAX)
}
