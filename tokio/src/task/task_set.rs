use crate::{
    future::poll_fn,
    runtime::Handle,
    task::{JoinError, JoinHandle},
    loom::sync::Mutex
};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// A collection of running tasks.
///
/// It simplifies joining and helps to propagate panics.
#[derive(Debug)]
pub struct TaskSet<T> {
    unfinished: Mutex<Vec<JoinHandle<T>>>,
    // Used for all spawns.
    handle: Handle,
}

impl<T> TaskSet<T> {
    /// Creates a new empty TaskSet.
    ///
    /// This function must be called inside the Tokio runtime context,
    /// otherwise it will panic.
    pub fn new() -> Self {
        TaskSet::new_with(Handle::current())
    }

    /// Creates a new empty TaskSet.
    ///
    /// Unlike the `new` function, it explicitly accepts runtime Handle
    /// and never panicks.
    pub fn new_with(handle: Handle) -> Self {
        TaskSet {
            unfinished: Mutex::new(Vec::new()),
            handle,
        }
    }

    /// Returns true if there are no unwaited tasks remaining.
    ///
    /// # Correctness
    /// If concurrent calls to `spawn` are possible, `true` returned can be stale
    /// (however `false` result is never stale).
    pub fn is_empty(&self) -> bool {
        self.unfinished.lock().is_empty()
    }

    /// Tries to wait for a finished task, with ability to handle failures.
    ///
    /// # Panics
    /// Panics if the set is empty.
    ///
    /// # Return value
    /// - Pending, if all tasks are still running
    /// - Ready(None), if the set is empty.
    /// - Ready(Some(Ok(x))), if a task resolved with resut x
    /// - Ready(Some(Err(e))), if a task failed with error err.
    pub fn try_poll_next_finished(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<T, JoinError>>> {
        if self.is_empty() {
            return Poll::Ready(None);
        }
        let f = self.unfinished.get_mut();
        for (i, task) in f.iter_mut().enumerate() {
            let task = Pin::new(task);
            if let Poll::Ready(result) = task.poll(cx) {
                f.swap_remove(i);
                return Poll::Ready(Some(result));
            }
        }
        Poll::Pending
    }

    /// Tries to wait for a finished task.
    /// This function panics on task failure and ignores cancelled tasks.
    ///
    /// # Return value
    /// - Ready(None), if the set is empty or all remaining tasks were cancelled.
    /// - Pending, if all tasks are still running
    /// - Ready(Some(x)), if a task resolved with resut x
    ///
    /// Unlike `try_poll_next_finished`, it is possible that None will be returned
    /// after Some(Pending), e.g. if all tasks get cancelled.
    pub fn poll_next_finished(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        loop {
            match self.try_poll_next_finished(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(x))) => return Poll::Ready(Some(x)),
                Poll::Ready(Some(Err(err))) => {
                    if err.is_cancelled() {
                        continue;
                    }
                    std::panic::resume_unwind(err.into_panic());
                }
                Poll::Ready(None) => return Poll::Ready(None),
            }
        }
    }

    /// Waits for a finished task, with ability to handle failures.
    ///
    /// # Return value
    /// - None, if the set is empty.
    /// - Some(Ok(x)), when a task resolves with resut x
    /// - Some(Err(e))), when a task failes with error err.
    pub async fn try_wait_next_finished(&mut self) -> Option<Result<T, JoinError>> {
        poll_fn(|cx| self.try_poll_next_finished(cx)).await
    }

    /// Waits for a next finished task.
    /// This function panics on task failure and ignores cancelled tasks.
    /// # Return value
    /// - None, when the set is empty.
    /// - Some(x), if a task resolved with result x
    pub async fn wait_next_finished(&mut self) -> Option<T> {
        poll_fn(|cx| self.poll_next_finished(cx)).await
    }

    /// Cancels all running tasks.
    pub fn cancel(&mut self) {
        let f = self.unfinished.get_mut();
        for t in f {
            t.abort();
        }
    }
}

impl<T: Send + 'static> TaskSet<T> {
    fn track(&self, handle: JoinHandle<T>) {
        let mut f = self.unfinished.lock();
        f.push(handle);
    }
    /// Spawns a future onto this task set
    pub fn spawn<F: Future<Output = T> + Send + 'static>(&mut self, fut: F) {
        let handle = self.handle.spawn(fut);
        self.track(handle);
    }

    /// Spawns a blocking task onto this task set
    pub fn spawn_blocking<R: FnOnce() -> T + Send + 'static>(&mut self, func: R) {
        let handle = self.handle.spawn_blocking(func);
        self.track(handle);
    }
}

impl TaskSet<()> {
    /// Waits for all running tasks to complete or propagates panic.
    /// `is_empty` returns true after this method completes.
    pub async fn wait_all(&mut self) {
        while self.wait_next_finished().await.is_some() {}
    }
}
