//! TaskSet API
//!
//! This module includes a TaskSet API for scoping the lifetimes of tasks and joining tasks in a
//! manner similar to [`futures_util::stream::FuturesUnordered`]

use futures_util::FutureExt;
use std::future::Future;
use std::task::{Context, Poll};
use tokio::runtime::Handle;
use tokio::task::{JoinError, JoinHandle};

/// A [`TaskSet`] is a mechanism for regulating the lifetimes of a set of asynchronous tasks.
///
/// # Task Lifetimes
/// Tasks spawned on a [`TaskSet`] live at most slightly longer than the set that spawns them.
/// The reason for this is that aborting a task is not an instantaneous operation.
///
/// Tasks could be running in another worker thread at the time they are cancelled, preventing
/// them from being canceled immediately. As a result, when you abort those tasks using the join
/// handles, Tokio marks those futures for cancellation. There are, however ways to await the
/// cancellation of tasks, but they only work in an async context.
///
/// If you need to cancel a set and wait for the cancellation to complete, use [`Self::shutdown`].
/// That function won't complete until *all* tasks have exited.
#[derive(Debug)]
pub struct TaskSet<T> {
    tasks: Vec<JoinHandle<T>>,
    handle: Handle,
}

impl<T> TaskSet<T> {
    /// Constructs a new TaskSet.
    ///
    /// # Panic
    /// Panics if invoked outside the context of a tokio runtime.
    pub fn new() -> Self {
        Self::new_with_handle(Handle::current())
    }

    /// Constructs a new TaskSet which will spawn tasks using the supplied handle.
    pub fn new_with_handle(handle: Handle) -> Self {
        Self {
            tasks: Vec::new(),
            handle,
        }
    }

    /// Returns the amount of unjoined tasks in the set.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Returns true if there are no tasks left in the set.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Consume the set, waiting for all tasks on it to complete.
    ///
    /// # Output
    /// The output of this function orders the results of tasks in the same order with which they
    /// were spawned.
    ///
    /// # Cancellation Safety
    /// When canceled, any tasks which have yet to complete will become detached, and the results
    /// of completed tasks will be discarded.
    ///
    /// # Examples
    /// ```
    /// # use tokio_util::taskset::TaskSet;
    /// # tokio_test::block_on(async {
    /// let mut set = TaskSet::new();
    ///
    /// const NUMS: [u8; 5] = [0, 1, 2, 3, 4];
    ///
    /// for x in NUMS.iter() {
    ///     set.spawn(async move { *x });
    /// }
    ///
    /// let joined: Vec<_> = set.join_all().await.into_iter().map(|x| x.unwrap()).collect();
    ///
    /// assert_eq!(NUMS, joined.as_slice());
    /// # });
    /// ```
    pub async fn join_all(mut self) -> Vec<Result<T, JoinError>> {
        let mut output = Vec::with_capacity(self.tasks.len());

        for task in self.tasks.iter_mut() {
            output.push(task.await)
        }

        self.tasks.clear();

        output
    }

    /// Join onto the next available task, removing it from the set.
    ///
    /// # Cancellation Safety
    /// This function is cancellation-safe. If dropped, it will be as if this was never called.
    ///
    /// # Examples
    /// ```
    /// # use tokio_util::taskset::TaskSet;
    /// # tokio_test::block_on(async {
    /// let mut set = TaskSet::new();
    ///
    /// set.spawn(async { 5u8 });
    /// set.spawn(std::future::pending());
    ///
    /// let joined = set.next_finished().await.unwrap().unwrap();
    /// assert_eq!(5u8, joined);
    /// # });
    /// ```
    pub async fn next_finished(&mut self) -> Option<Result<T, JoinError>> {
        if self.tasks.is_empty() {
            None
        } else {
            futures_util::future::poll_fn(|cx| self.poll_next_finished(cx)).await
        }
    }

    /// Shutdown the task set, cancelling all running futures and returning the results from
    /// joining the tasks.
    ///
    /// # Output
    /// Like [`Self::join_all`], the ordering of tasks is preserved. To check if a task was
    /// cancelled, use [`JoinError::is_cancelled`].
    ///
    /// # Cancellation Safety
    /// When cancelled, tasks will still have been aborted, but you will have no way of waiting
    /// for the aborts to complete.
    ///
    /// # Examples
    /// ## Verifying Task Shutdown
    /// ```
    /// # use tokio_util::taskset::TaskSet;
    /// # use std::sync::Arc;
    /// # use std::sync::atomic::{AtomicU64, Ordering};
    /// # tokio_test::block_on(async {
    /// const NUM_TASKS: u64 = 1024;
    ///
    /// let counter = Arc::new(AtomicU64::default());
    ///
    /// let mut set: TaskSet<()> = TaskSet::new();
    ///
    /// for _ in 0..NUM_TASKS {
    ///     let guard = scopeguard::guard(counter.clone(), |x| {
    ///         x.fetch_add(1, Ordering::SeqCst);
    ///     });
    ///
    ///     set.spawn(async move {
    ///         let _guard = guard;
    ///         // we must never surrender to the forces of time
    ///         let _: () = std::future::pending().await;
    ///     });
    /// }
    ///
    /// for e in set.shutdown().await.into_iter().map(|x| x.unwrap_err()) {
    ///     assert!(e.is_cancelled());
    /// }
    ///
    /// assert_eq!(NUM_TASKS, counter.load(Ordering::Relaxed));
    /// # });
    /// ```
    /// ## Cancellation
    /// ```
    /// # use tokio_util::taskset::TaskSet;
    /// # use std::sync::Arc;
    /// # use std::sync::atomic::{AtomicU64, Ordering};
    /// # tokio_test::block_on(async {
    /// const NUM_TASKS: u64 = 64;
    ///
    /// let counter = Arc::new(AtomicU64::default());
    ///
    /// let mut set: TaskSet<()> = TaskSet::new();
    ///
    /// for _ in 0..NUM_TASKS {
    ///     let guard = scopeguard::guard(counter.clone(), |x| {
    ///         x.fetch_add(1, Ordering::SeqCst);
    ///     });
    ///
    ///     set.spawn(async move {
    ///         let _guard = guard;
    ///         let _: () = std::future::pending().await;
    ///     });
    /// }
    ///
    /// let shutdown = set.shutdown();
    ///
    /// // cancel task
    /// drop(shutdown);
    ///
    /// tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
    /// assert_eq!(NUM_TASKS, counter.load(Ordering::Relaxed));
    /// # });
    /// ```
    pub fn shutdown(self) -> impl Future<Output = Vec<Result<T, JoinError>>> {
        // abort all tasks *before* the cancellable future
        self.abort_all();
        // this part can be cancelled without us leaking tasks
        self.join_all()
    }

    /// Poll the next finished task.
    pub fn poll_next_finished(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<T, JoinError>>> {
        // implementation based off of futures_util::select_all
        // o(n) due to scan in search of ready tasks

        if self.tasks.is_empty() {
            Poll::Ready(None)
        } else {
            let item =
                self.tasks
                    .iter_mut()
                    .enumerate()
                    .find_map(|(i, f)| match f.poll_unpin(cx) {
                        Poll::Pending => None,
                        Poll::Ready(e) => Some((i, e)),
                    });
            match item {
                Some((idx, res)) => {
                    let _ = self.tasks.swap_remove(idx);
                    Poll::Ready(Some(res))
                }
                None => Poll::Pending,
            }
        }
    }

    fn abort_all(&self) {
        for task in self.tasks.iter() {
            task.abort();
        }
    }
}

impl<T> TaskSet<T>
where
    T: Send + 'static,
{
    /// Spawn a task onto the set.
    pub fn spawn<F>(&mut self, f: F)
    where
        F: Future<Output = T> + Send + 'static,
    {
        let task = self.handle.spawn(f);

        self.tasks.push(task);
    }
}

impl<T> Default for TaskSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Tasks are aborted on drop.
///
/// Tasks aborted this way are not instantly cancelled, and the time required depends on a lot of
/// things. If you need to make sure that tasks are immediately cancelled, use [`Self::shutdown`].
impl<T> Drop for TaskSet<T> {
    fn drop(&mut self) {
        self.abort_all();
    }
}

#[cfg(test)]
mod tests {
    const NUM_TASKS: u64 = 64;

    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    #[tokio::test(flavor = "current_thread")]
    async fn test_current_thread_abort() {
        test_abort().await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_multi_thread_abort() {
        test_abort().await
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_current_thread_drop() {
        test_drop().await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_multi_thread_drop() {
        test_drop().await
    }

    /// Ensure that the future returned by [`TaskSet::shutdown`] won't complete until all tasks
    /// have been dropped.
    async fn test_abort() {
        let counter = Arc::new(AtomicU64::default());

        let mut set: TaskSet<()> = TaskSet::new();

        for _ in 0..NUM_TASKS {
            let guard = scopeguard::guard(counter.clone(), |x| {
                x.fetch_add(1, Ordering::SeqCst);
            });

            set.spawn(async move {
                let _guard = guard;
                let _: () = std::future::pending().await;
            });
        }

        for e in set.shutdown().await.into_iter().map(|x| x.unwrap_err()) {
            assert!(e.is_cancelled());
        }

        assert_eq!(NUM_TASKS, counter.load(Ordering::Relaxed));
    }

    /// Ensure that dropping the TaskSet successfully aborts all tasks on the set.
    async fn test_drop() {
        let counter = Arc::new(AtomicU64::default());

        let mut set = TaskSet::new();

        for _ in 0..NUM_TASKS {
            let guard = scopeguard::guard(counter.clone(), |x| {
                x.fetch_add(1, Ordering::SeqCst);
            });

            set.spawn(async move {
                let _guard = guard;
                let _: () = std::future::pending().await;
            });
        }

        drop(set);

        // if these simple tasks haven't been dropped yet, we have a problem
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        assert_eq!(NUM_TASKS, counter.load(Ordering::Relaxed));
    }
}
