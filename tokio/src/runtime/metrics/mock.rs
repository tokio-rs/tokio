//! This file contains mocks of the types in src/runtime/metrics

pub(crate) struct SchedulerMetrics {}

impl SchedulerMetrics {
    pub(crate) fn new() -> Self {
        Self {}
    }

    /// Increment the number of tasks scheduled externally
    pub(crate) fn inc_remote_schedule_count(&self) {}
}

#[derive(Debug)]
pub(crate) struct Histogram {}

pub(crate) struct HistogramBatch {}

#[derive(Debug, Clone, Default)]
pub(crate) struct HistogramBuilder {}

impl HistogramBuilder {
    pub(crate) fn build(&self) -> Histogram {
        Histogram {}
    }
}

impl HistogramBatch {
    pub(crate) fn from_histogram(_histogram: &Histogram) -> HistogramBatch {
        HistogramBatch {}
    }

    pub(crate) fn submit(&self, _histogram: &Histogram) {}

    pub(crate) fn measure(&mut self, _value: u64, _count: u64) {}
}
