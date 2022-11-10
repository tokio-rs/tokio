use crate::runtime::task::{self, Task};
use crate::time::Clock;

/// `task::Schedule` implementation that does nothing (except some bookkeeping
/// in test-util builds). This is unique to the blocking scheduler as tasks
/// scheduled are not really futures but blocking operations.
///
/// We avoid storing the task by forgetting it in `bind` and re-materializing it
/// in `release`.
pub(crate) struct BlockingSchedule {
    #[cfg(feature = "test-util")]
    clock: Clock,
}

impl BlockingSchedule {
    pub(crate) fn new() -> Self {
        BlockingSchedule {
            #[cfg(feature = "test-util")]
            clock: crate::time::inhibit_auto_advance(),
        }
    }
}

impl task::Schedule for BlockingSchedule {
    fn release(&self, _task: &Task<Self>) -> Option<Task<Self>> {
        #[cfg(feature = "test-util")]
        {
            self.clock.allow_auto_advance();
        }
        None
    }

    fn schedule(&self, _task: task::Notified<Self>) {
        unreachable!();
    }
}
