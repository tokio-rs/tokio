use crate::runtime::task::{self, Task};

/// `task::Schedule` implementation that does nothing. This is unique to the
/// blocking scheduler as tasks scheduled are not really futures but blocking
/// operations.
///
/// We avoid storing the task by forgetting it in `bind` and re-materializing it
/// in `release.
pub(super) struct NoopSchedule;

impl task::Schedule for NoopSchedule {
    fn bind(_task: Task<Self>) -> NoopSchedule {
        // Do nothing w/ the task
        NoopSchedule
    }

    fn release(&self, _task: &Task<Self>) -> Option<Task<Self>> {
        None
    }

    fn schedule(&self, _task: task::Notified<Self>) {
        unreachable!();
    }
}
