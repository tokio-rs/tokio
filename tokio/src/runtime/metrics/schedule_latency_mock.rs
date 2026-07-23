//! A mock implementation of the types in `schedule_latency`. These types
//! are zero-sized so that the size of types using them are not increased
//! unless schedule latency tracking is explicitly enabled.

use std::time::Instant;

#[derive(Copy, Clone)]
pub(crate) struct ScheduleLatencyInstant();

impl ScheduleLatencyInstant {
    pub(crate) fn new(_runtime_start: Option<Instant>) -> Self {
        Self()
    }

    pub(crate) fn prepare(self, _runtime_start: Option<Instant>) -> Option<ScheduleLatencyContext> {
        None
    }
}

pub(crate) struct ScheduleLatencyContext {
    _private: (),
}
