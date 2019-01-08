use task::Task;

use std::fmt;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// A list of tasks that have been polled but not completed.
///
/// Every worker has its own task registry. If a worker thread polls a task for the first time, it
/// is added to the registry. If it completes a task, it is removed from the registry.
///
/// When the thread pool shuts down, we can see which tasks are in progress but not completed by
/// inspecting the task registries. Dropping a registry aborts all tasks registered in it, so we
/// can just drop each worker's registry in order to abort all incomplete tasks.
pub(crate) struct Registry {
    /// The list of registered tasks.
    tasks: Vec<*const Task>,
}

impl Registry {
    /// Returns a new task registry.
    pub fn new() -> Registry {
        Registry {
            tasks: Vec::new(),
        }
    }

    /// Adds a task to the registry.
    #[inline]
    pub fn add(&mut self, task: &Arc<Task>) {
        // Remember the index in the `Vec` so that we can later remove the task in O(1) time.
        task.reg_index.store(self.tasks.len(), Ordering::Release);
        self.tasks.push(Arc::into_raw(task.clone()));
    }

    /// Removes a task from the registry.
    #[inline]
    pub fn remove(&mut self, task: &Arc<Task>) {
        let index = task.reg_index.load(Ordering::Acquire);
        assert!(ptr::eq(self.tasks[index], &**task));

        unsafe {
            drop(Arc::from_raw(self.tasks.swap_remove(index)));

            // By doing `swap_remove()` we might've changed the position of another task in the
            // `Vec`, so we need to update its index.
            if index < self.tasks.len() {
                (*self.tasks[index]).reg_index.store(index, Ordering::Release);
            }
        }
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        // Abort all tasks in the registry.
        for raw in self.tasks.drain(..) {
            unsafe {
                let task = Arc::from_raw(raw);
                task.abort();
            }
        }
    }
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad("Registry")
    }
}
