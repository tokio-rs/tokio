//! This file contains mocks of the types in src/runtime/metrics

pub(crate) struct SchedulerMetrics {}

pub(crate) struct WorkerMetrics {}

pub(crate) struct MetricsBatch {}

#[derive(Clone, Default)]
pub(crate) struct HistogramBuilder {}

impl SchedulerMetrics {
    pub(crate) fn new() -> Self {
        Self {}
    }

    /// Increment the number of tasks scheduled externally
    pub(crate) fn inc_remote_schedule_count(&self) {}
}

impl WorkerMetrics {
    pub(crate) fn new() -> Self {
        Self {}
    }

    pub(crate) fn from_config(config: &crate::runtime::Config) -> Self {
        // Prevent the dead-code warning from being triggered
        let _ = &config.metrics_poll_count_histogram;
        Self::new()
    }

    pub(crate) fn set_queue_depth(&self, _len: usize) {}
}

impl MetricsBatch {
    pub(crate) fn new(_: &WorkerMetrics) -> Self {
        Self {}
    }

    pub(crate) fn submit(&mut self, _to: &WorkerMetrics) {}
    pub(crate) fn about_to_park(&mut self) {}
    pub(crate) fn inc_local_schedule_count(&mut self) {}
    pub(crate) fn start_processing_scheduled_tasks(&mut self) {}
    pub(crate) fn end_processing_scheduled_tasks(&mut self) {}
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
