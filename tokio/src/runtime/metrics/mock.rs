//! This file contains mocks of the types in src/runtime/metrics

pub(crate) struct RuntimeMetrics {}

pub(crate) struct WorkerMetrics {}

pub(crate) struct MetricsBatch {}

impl RuntimeMetrics {
    pub(crate) fn new(_worker_threads: usize) -> Self {
        Self {}
    }

    /// Increment the number of tasks scheduled externally
    pub(crate) fn inc_remote_schedule_count(&self) {}

    pub(crate) fn worker(&self, _index: usize) -> &WorkerMetrics {
        &WorkerMetrics {}
    }
}

impl WorkerMetrics {
    pub(crate) fn incr_stolen_count(&self, _n: u16) {}
}

impl MetricsBatch {
    pub(crate) fn new(_my_index: usize) -> Self {
        Self {}
    }

    pub(crate) fn submit(&mut self, _to: &RuntimeMetrics) {}
    pub(crate) fn about_to_park(&mut self) {}
    pub(crate) fn returned_from_park(&mut self) {}
    pub(crate) fn incr_poll_count(&mut self) {}
    pub(crate) fn inc_local_schedule_count(&mut self) {}
}

cfg_rt_multi_thread! {
    impl MetricsBatch {
        pub(crate) fn incr_steal_count(&mut self, _by: u16) {}
        pub(crate) fn incr_overflow_count(&mut self, _by: u16) {}
    }
}
