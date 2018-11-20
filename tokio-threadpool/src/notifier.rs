use pool::Pool;
use task::Task;

use std::mem;
use std::ops;
use std::sync::Arc;

use futures::executor::Notify;

/// Implements the future `Notify` API.
///
/// This is how external events are able to signal the task, informing it to try
/// to poll the future again.
#[derive(Debug)]
pub(crate) struct Notifier {
    pub pool: Arc<Pool>,
}

/// A guard that ensures that the inner value gets forgotten.
#[derive(Debug)]
struct Forget<T>(Option<T>);

impl Notify for Notifier {
    fn notify(&self, id: usize) {
        trace!("Notifier::notify; id=0x{:x}", id);

        unsafe {
            let ptr = id as *const Task;

            // We did not actually take ownership of the `Arc` in this function
            // so we must ensure that the Arc is forgotten.
            let task = Forget::new(Arc::from_raw(ptr));

            // TODO: Unify this with Task::notify
            if task.schedule() {
                // TODO: Check if the pool is still running
                //
                // Bump the ref count
                let task = task.clone();

                let _ = self.pool.submit(task, &self.pool);
            }
        }
    }

    fn clone_id(&self, id: usize) -> usize {
        let ptr = id as *const Task;

        // This function doesn't actually get a strong ref to the task here.
        // However, the only method we have to convert a raw pointer -> &Arc<T>
        // is to call `Arc::from_raw` which returns a strong ref. So, to
        // maintain the invariants, `t1` has to be forgotten. This prevents the
        // ref count from being decremented.
        let t1 = Forget::new(unsafe { Arc::from_raw(ptr) });

        // The clone is forgotten so that the fn exits without decrementing the ref
        // count. The caller of `clone_id` ensures that `drop_id` is called when
        // the ref count needs to be decremented.
        let _ = Forget::new(t1.clone());

        id
    }

    fn drop_id(&self, id: usize) {
        unsafe {
            let ptr = id as *const Task;
            let _ = Arc::from_raw(ptr);
        }
    }
}

// ===== impl Forget =====

impl<T> Forget<T> {
    fn new(t: T) -> Self {
        Forget(Some(t))
    }
}

impl<T> ops::Deref for Forget<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0.as_ref().unwrap()
    }
}

impl<T> Drop for Forget<T> {
    fn drop(&mut self) {
        mem::forget(self.0.take());
    }
}
