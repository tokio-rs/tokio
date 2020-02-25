use crate::task::{self, Task};

/// `task::Schedule` implementation that does nothing. This is unique to the
/// blocking scheduler as tasks scheduled are not really futures but blocking
/// operations.
pub(super) struct NoopSchedule;

impl task::Schedule for NoopSchedule {
    fn bind(_task: &Task<Self>) -> NoopSchedule {
        NoopSchedule
    }

    fn release(&self, _task: Task<Self>) {}

    fn schedule(&self, _task: task::Notified<Self>) {
        unreachable!();
    }
}

impl task::ScheduleSendOnly for NoopSchedule {}
