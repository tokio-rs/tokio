use crate::loom::sync::Mutex;
use crate::runtime::task::Task;
use crate::util::linked_list::{Link, LinkedList};

pub(crate) struct OwnedTasks<S: 'static> {
    list: Mutex<LinkedList<Task<S>, <Task<S> as Link>::Target>>,
}

impl<S: 'static> OwnedTasks<S> {
    pub(crate) fn new() -> Self {
        Self {
            list: Mutex::new(LinkedList::new()),
        }
    }

    pub(crate) fn push_front(&self, task: Task<S>) {
        self.list.lock().push_front(task);
    }

    pub(crate) fn pop_back(&self) -> Option<Task<S>> {
        self.list.lock().pop_back()
    }

    /// The caller must ensure that if the provided task is stored in a
    /// linked list, then it is in this linked list.
    pub(crate) unsafe fn remove(&self, task: &Task<S>) -> Option<Task<S>> {
        self.list.lock().remove(task.header().into())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.list.lock().is_empty()
    }
}
