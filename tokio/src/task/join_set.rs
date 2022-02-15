use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::runtime::Handle;
use crate::task::{JoinError, JoinHandle, LocalSet};
use crate::util::IdleNotifiedSet;

/// A collection of tasks spawned on a Tokio runtime.
///
/// A `JoinSet` can be used to await the completion of some or all of the tasks
/// in the set. The set is not ordered, and the tasks will be returned in the
/// order they complete.
///
/// All of the tasks must have the same return type `T`.
///
/// When the `JoinSet` is dropped, all tasks in the `JoinSet` are immediately aborted.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// # Examples
///
/// Spawn multiple tasks and wait for them.
///
/// ```
/// use tokio::task::JoinSet;
///
/// #[tokio::main]
/// async fn main() {
///     let mut set = JoinSet::new();
///
///     for i in 0..10 {
///         set.spawn(async move { i });
///     }
///
///     let mut seen = [false; 10];
///     while let Some(res) = set.join_one().await.unwrap() {
///         seen[res] = true;
///     }
///
///     for i in 0..10 {
///         assert!(seen[i]);
///     }
/// }
/// ```
///
/// [unstable]: crate#unstable-features
pub struct JoinSet<T> {
    inner: IdleNotifiedSet<JoinHandle<T>>,
}

impl<T> JoinSet<T> {
    /// Create a new `JoinSet`.
    pub fn new() -> Self {
        Self {
            inner: IdleNotifiedSet::new(),
        }
    }

    /// Returns the number of tasks currently in the `JoinSet`.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns whether the `JoinSet` is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<T: 'static> JoinSet<T> {
    /// Spawn the provided task on the `JoinSet`.
    ///
    /// # Panics
    ///
    /// This method panics if called outside of a Tokio runtime.
    pub fn spawn<F>(&mut self, task: F)
    where
        F: Future<Output = T>,
        F: Send + 'static,
        T: Send,
    {
        self.insert(crate::spawn(task));
    }

    /// Spawn the provided task on the provided runtime and store it in this `JoinSet`.
    pub fn spawn_on<F>(&mut self, task: F, handle: &Handle)
    where
        F: Future<Output = T>,
        F: Send + 'static,
        T: Send,
    {
        self.insert(handle.spawn(task));
    }

    /// Spawn the provided task on the current [`LocalSet`] and store it in this `JoinSet`.
    ///
    /// # Panics
    ///
    /// This method panics if it is called outside of a `LocalSet`.
    ///
    /// [`LocalSet`]: crate::task::LocalSet
    pub fn spawn_local<F>(&mut self, task: F)
    where
        F: Future<Output = T>,
        F: 'static,
    {
        self.insert(crate::task::spawn_local(task));
    }

    /// Spawn the provided task on the provided [`LocalSet`] and store it in this `JoinSet`.
    ///
    /// [`LocalSet`]: crate::task::LocalSet
    pub fn spawn_local_on<F>(&mut self, task: F, local_set: &LocalSet)
    where
        F: Future<Output = T>,
        F: 'static,
    {
        self.insert(local_set.spawn_local(task));
    }

    fn insert(&mut self, jh: JoinHandle<T>) {
        let mut entry = self.inner.insert_idle(jh);

        // Set the waker that is notified when the task completes.
        entry.with_value_and_context(|jh, ctx| jh.set_join_waker(ctx.waker()));
    }

    /// Waits until one of the tasks in the set completes and returns its output.
    ///
    /// Returns `None` if the set is empty.
    ///
    /// # Cancel Safety
    ///
    /// This method is cancel safe. If `join_one` is used as the event in a `tokio::select!`
    /// statement and some other branch completes first, it is guaranteed that no tasks were
    /// removed from this `JoinSet`.
    pub async fn join_one(&mut self) -> Result<Option<T>, JoinError> {
        crate::future::poll_fn(|cx| self.poll_join_one(cx)).await
    }

    /// Aborts all tasks and waits for them to finish shutting down.
    ///
    /// Calling this method is equivalent to calling [`abort_all`] and then calling [`join_one`] in
    /// a loop until it returns `Ok(None)`.
    ///
    /// This method ignores any panics in the tasks shutting down. When this call returns, the
    /// `JoinSet` will be empty.
    ///
    /// [`abort_all`]: fn@Self::abort_all
    /// [`join_one`]: fn@Self::join_one
    pub async fn shutdown(&mut self) {
        self.abort_all();
        while self.join_one().await.transpose().is_some() {}
    }

    /// Aborts all tasks on this `JoinSet`.
    ///
    /// This does not remove the tasks from the `JoinSet`. To wait for the tasks to complete
    /// cancellation, you should call `join_one` in a loop until the `JoinSet` is empty.
    pub fn abort_all(&mut self) {
        self.inner.for_each(|jh| jh.abort());
    }

    /// Removes all tasks from this `JoinSet` without aborting them.
    ///
    /// The tasks removed by this call will continue to run in the background even if the `JoinSet`
    /// is dropped.
    pub fn detach_all(&mut self) {
        self.inner.drain(drop);
    }

    /// Polls for one of the tasks in the set to complete.
    ///
    /// If this returns `Poll::Ready(Ok(Some(_)))` or `Poll::Ready(Err(_))`, then the task that
    /// completed is removed from the set.
    ///
    /// When the method returns `Poll::Pending`, the `Waker` in the provided `Context` is scheduled
    /// to receive a wakeup when a task in the `JoinSet` completes. Note that on multiple calls to
    /// `poll_join_one`, only the `Waker` from the `Context` passed to the most recent call is
    /// scheduled to receive a wakeup.
    ///
    /// # Returns
    ///
    /// This function returns:
    ///
    ///  * `Poll::Pending` if the `JoinSet` is not empty but there is no task whose output is
    ///     available right now.
    ///  * `Poll::Ready(Ok(Some(value)))` if one of the tasks in this `JoinSet` has completed. The
    ///    `value` is the return value of one of the tasks that completed.
    ///  * `Poll::Ready(Err(err))` if one of the tasks in this `JoinSet` has panicked or been
    ///     aborted.
    ///  * `Poll::Ready(Ok(None))` if the `JoinSet` is empty.
    ///
    /// Note that this method may return `Poll::Pending` even if one of the tasks has completed.
    /// This can happen if the [coop budget] is reached.
    ///
    /// [coop budget]: crate::task#cooperative-scheduling
    fn poll_join_one(&mut self, cx: &mut Context<'_>) -> Poll<Result<Option<T>, JoinError>> {
        // The call to `pop_notified` moves the entry to the `idle` list. It is moved back to
        // the `notified` list if the waker is notified in the `poll` call below.
        let mut entry = match self.inner.pop_notified(cx.waker()) {
            Some(entry) => entry,
            None => {
                if self.is_empty() {
                    return Poll::Ready(Ok(None));
                } else {
                    // The waker was set by `pop_notified`.
                    return Poll::Pending;
                }
            }
        };

        let res = entry.with_value_and_context(|jh, ctx| Pin::new(jh).poll(ctx));

        if let Poll::Ready(res) = res {
            entry.remove();
            Poll::Ready(Some(res).transpose())
        } else {
            // A JoinHandle generally won't emit a wakeup without being ready unless
            // the coop limit has been reached. We yield to the executor in this
            // case.
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

impl<T> Drop for JoinSet<T> {
    fn drop(&mut self) {
        self.inner.drain(|join_handle| join_handle.abort());
    }
}

impl<T> fmt::Debug for JoinSet<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinSet").field("len", &self.len()).finish()
    }
}

impl<T> Default for JoinSet<T> {
    fn default() -> Self {
        Self::new()
    }
}
