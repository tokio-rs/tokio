use crate::task::{Schedule, ScheduleSendOnly, Task};

/// `task::Schedule` implementation that does nothing. This is unique to the
/// blocking scheduler as tasks scheduled are not really futures but blocking
/// operations.
pub(super) struct NoopSchedule;

impl Schedule for NoopSchedule {
    fn bind(&self, _task: &Task<Self>) {}

    fn release(&self, _task: Task<Self>) {}

    fn release_local(&self, _task: &Task<Self>) {}

    fn schedule(&self, _task: Task<Self>) {
        unreachable!();
    }
}

impl ScheduleSendOnly for NoopSchedule {}
