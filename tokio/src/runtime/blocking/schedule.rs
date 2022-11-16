use crate::runtime::task::{self, Task};
#[cfg(feature = "test-util")]
use crate::runtime::{scheduler, Handle};

/// `task::Schedule` implementation that does nothing (except some bookkeeping
/// in test-util builds). This is unique to the blocking scheduler as tasks
/// scheduled are not really futures but blocking operations.
///
/// We avoid storing the task by forgetting it in `bind` and re-materializing it
/// in `release`.
pub(crate) struct BlockingSchedule {
    #[cfg(feature = "test-util")]
    handle: Handle,
}

impl BlockingSchedule {
    pub(crate) fn new() -> Self {
        BlockingSchedule {
            #[cfg(feature = "test-util")]
            handle: {
                let handle = Handle::current();
                match &handle.inner {
                    scheduler::Handle::CurrentThread(handle) => {
                        handle.driver.clock.inhibit_auto_advance();
                    }
                    #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                    scheduler::Handle::MultiThread(_) => {}
                }
                handle
            },
        }
    }
}

impl task::Schedule for BlockingSchedule {
    fn release(&self, _task: &Task<Self>) -> Option<Task<Self>> {
        #[cfg(feature = "test-util")]
        {
            match &self.handle.inner {
                scheduler::Handle::CurrentThread(handle) => {
                    handle.driver.clock.allow_auto_advance();
                    handle.driver.unpark();
                }
                #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                scheduler::Handle::MultiThread(_) => {}
            }
        }
        None
    }

    fn schedule(&self, _task: task::Notified<Self>) {
        unreachable!();
    }
}
