use crate::task::{Schedule, Task};

use loom::sync::Notify;
use std::collections::VecDeque;
use std::sync::Mutex;

pub(crate) struct LoomSchedule {
    notify: Notify,
    pending: Mutex<VecDeque<Option<Task<Self>>>>,
}

impl LoomSchedule {
    pub(crate) fn new() -> LoomSchedule {
        LoomSchedule {
            notify: Notify::new(),
            pending: Mutex::new(VecDeque::new()),
        }
    }

    pub(crate) fn push_task(&self, task: Task<Self>) {
        self.schedule(task);
    }

    pub(crate) fn recv(&self) -> Option<Task<Self>> {
        loop {
            if let Some(task) = self.pending.lock().unwrap().pop_front() {
                return task;
            }

            self.notify.wait();
        }
    }
}

impl Schedule for LoomSchedule {
    fn bind(&self, _task: &Task<Self>) {}

    fn release(&self, task: Task<Self>) {
        self.release_local(&task);
    }

    fn release_local(&self, _task: &Task<Self>) {
        self.pending.lock().unwrap().push_back(None);
        self.notify.notify();
    }

    fn schedule(&self, task: Task<Self>) {
        self.pending.lock().unwrap().push_back(Some(task));
        self.notify.notify();
    }
}
