use crate::pool::Pool;
use crate::task::Task;

use futures_util::task::ArcWake;
use std::sync::Arc;

/// Implements the future `Waker` API.
///
/// This is how external events are able to signal the task, informing it to try
/// to poll the future again.
#[derive(Debug)]
pub(crate) struct Waker {
    pub pool: Arc<Pool>,
    pub task: Arc<Task>,
}

unsafe impl Send for Waker {}
unsafe impl Sync for Waker {}

impl ArcWake for Waker {
    fn wake_by_ref(me: &Arc<Self>) {
        Task::schedule(&me.task, &me.pool);
    }
}
