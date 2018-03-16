use futures::task::AtomicTask;
#[cfg(feature = "unstable-futures")]
use futures2;

#[derive(Debug)]
pub(crate) struct ShutdownTask {
    pub(crate) task1: AtomicTask,

    #[cfg(feature = "unstable-futures")]
    pub(crate) task2: futures2::task::AtomicWaker,
}

impl ShutdownTask {
    #[cfg(not(feature = "unstable-futures"))]
    pub(crate) fn notify(&self) {
        self.task1.notify();
    }

    #[cfg(feature = "unstable-futures")]
    pub(crate) fn notify(&self) {
        self.task1.notify();
        self.task2.wake();
    }
}
