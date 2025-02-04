#[cfg(feature = "test-util")]
use crate::runtime::scheduler;
use crate::runtime::task::{self, Task};
use crate::runtime::Handle;
#[cfg(tokio_unstable)]
use crate::runtime::{OptionalTaskHooksFactory, OptionalTaskHooksFactoryRef};

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
    hooks_factory: OptionalTaskHooksFactory,
}

impl BlockingSchedule {
    #[cfg_attr(not(feature = "test-util"), allow(unused_variables))]
    pub(crate) fn new(handle: &Handle) -> Self {
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
            hooks_factory: handle.inner.hooks_factory(),
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
    fn hooks_factory(&self) -> OptionalTaskHooksFactory {
        self.hooks_factory.clone()
    }

    #[cfg(tokio_unstable)]
    fn hooks_factory_ref(&self) -> OptionalTaskHooksFactoryRef<'_> {
        self.hooks_factory.as_ref().map(AsRef::as_ref)
    }
}
