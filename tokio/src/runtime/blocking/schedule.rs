#[cfg(feature = "test-util")]
use crate::runtime::scheduler;
use crate::runtime::task::{self, Task};
use crate::runtime::Handle;
#[cfg(tokio_unstable)]
use crate::runtime::{TaskCallback, TaskMeta};

/// `task::Schedule` implementation that does nothing (except some bookkeeping
/// in test-util builds). This is unique to the blocking scheduler as tasks
/// scheduled are not really futures but blocking operations.
///
/// We avoid storing the task by forgetting it in `bind` and re-materializing it
/// in `release`.
pub(crate) struct BlockingSchedule {
    #[cfg(feature = "test-util")]
    handle: Handle,
    #[cfg(tokio_unstable)]
    task_terminate_callback: Option<TaskCallback>,
}

impl BlockingSchedule {
    #[cfg_attr(not(feature = "test-util"), allow(unused_variables))]
    pub(crate) fn new(handle: &Handle, #[cfg(tokio_unstable)] run_task_hooks: bool) -> Self {
        #[cfg(feature = "test-util")]
        {
            match &handle.inner {
                scheduler::Handle::CurrentThread(handle) => {
                    handle.driver.clock.inhibit_auto_advance();
                }
                #[cfg(feature = "rt-multi-thread")]
                scheduler::Handle::MultiThread(_) => {}
            }
        }
        BlockingSchedule {
            #[cfg(feature = "test-util")]
            handle: handle.clone(),
            #[cfg(tokio_unstable)]
            task_terminate_callback: if run_task_hooks {
                handle.inner.hooks().task_terminate_callback.clone()
            } else {
                None
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
                #[cfg(feature = "rt-multi-thread")]
                scheduler::Handle::MultiThread(_) => {}
            }
        }
        None
    }

    fn schedule(&self, _task: task::Notified<Self>) {
        unreachable!();
    }

    #[cfg(tokio_unstable)]
    fn task_terminate_callback(&self, meta: &mut TaskMeta<'_>) {
        if let Some(task_terminate_callback) = &self.task_terminate_callback {
            task_terminate_callback(meta);
        }
    }
}
