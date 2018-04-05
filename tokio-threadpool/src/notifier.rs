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
        let ptr = id as *const Task;

        // This function doesn't actually get a strong ref to the task here.
        // However, the only method we have to convert a raw pointer -> &Arc<T>
        // is to call `Arc::from_raw` which returns a strong ref. So, to
        // maintain the invariants, `t1` has to be forgotten. This prevents the
        // ref count from being decremented.
        let t1 = unsafe { Arc::from_raw(ptr) };
        let t2 = t1.clone();

        mem::forget(t1);

        // t2 is forgotten so that the fn exits without decrementing the ref
        // count. The caller of `clone_id` ensures that `drop_id` is called when
        // the ref count needs to be decremented.
        mem::forget(t2);

        id
    }

    fn drop_id(&self, id: usize) {
        unsafe {
            let ptr = id as *const Task;
            let _ = Arc::from_raw(ptr);
        }
    }
}
