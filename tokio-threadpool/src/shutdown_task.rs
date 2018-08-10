use futures::task::AtomicTask;

#[derive(Debug)]
pub(crate) struct ShutdownTask {
    pub task: AtomicTask,
}

impl ShutdownTask {
    pub fn notify(&self) {
        self.task.notify();
    }
}
