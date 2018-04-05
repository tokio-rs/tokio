use pool::Pool;
use task::Task;

use std::mem;
use std::sync::{Arc, Weak};

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

        unsafe {
            let ptr = id as *const Task;
            let task = Arc::from_raw(ptr);

            if task.schedule() {
                // TODO: Check if the pool is still running
                //
                // Bump the ref count
                let task = task.clone();

                if let Some(inner) = self.inner.upgrade() {
                    let _ = inner.submit(task, &inner);
                }
            }

            // We did not actually take ownership of the `Arc` in this function.
            mem::forget(task);
        }
    }

    fn clone_id(&self, id: usize) -> usize {
        unsafe {
            let ptr = id as *const Task;

            let t1 = Arc::from_raw(ptr);
            let t2 = t1.clone();

            // Forget both handles as we don't want any ref decs
            mem::forget(t1);
            mem::forget(t2);
        }

        id
    }

    fn drop_id(&self, id: usize) {
        unsafe {
            let ptr = id as *const Task;
            let _ = Arc::from_raw(ptr);
        }
    }
}
