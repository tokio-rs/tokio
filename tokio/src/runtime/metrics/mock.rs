//! This file contains mocks of the types in src/runtime/metrics/metrics.rs

pub(crate) struct RuntimeMetrics {
}

impl RuntimeMetrics {
    pub(crate) fn new(_worker_threads: usize) -> Self {
        Self {
        }
    }
}



pub(crate) struct WorkerMetricsBatcher {
}

impl WorkerMetricsBatcher {
    pub(crate) fn new(_my_index: usize) -> Self {
        Self {
        }
    }

    pub(crate) fn submit(&mut self, _to: &RuntimeMetrics) {
    }

    pub(crate) fn incr_park_count(&mut self) {
    }

    pub(crate) fn incr_steal_count(&mut self, _by: u16) {
    }

    pub(crate) fn incr_poll_count(&mut self) {
    }
}
