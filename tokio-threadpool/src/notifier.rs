use pool::Pool;
use task::Task;

use std::mem;
use std::sync::Weak;

use futures::executor::Notify;

/// Implements the future `Notify` API.
///
/// This is how external events are able to signal the task, informing it to try
/// to poll the future again.
#[derive(Debug)]
pub(crate) struct Notifier {
    pub inner: Weak<Pool>,
}

impl Notify for Notifier {
    fn notify(&self, id: usize) {
        trace!("Notifier::notify; id=0x{:x}", id);

        let id = id as usize;
        let task = unsafe { Task::from_notify_id_ref(&id) };

        if !task.schedule() {
            trace!("    -> task already scheduled");
            // task is already scheduled, there is nothing more to do
            return;
        }

        // TODO: Check if the pool is still running

        // Bump the ref count
        let task = task.clone();

        if let Some(inner) = self.inner.upgrade() {
            let _ = inner.submit(task, &inner);
        }
    }

    fn clone_id(&self, id: usize) -> usize {
        unsafe {
            let handle = Task::from_notify_id_ref(&id);
            mem::forget(handle.clone());
        }

        id
    }

    fn drop_id(&self, id: usize) {
        unsafe {
            let _ = Task::from_notify_id(id);
        }
    }
}
