use crate::runtime::task::{Id, RawTask};

use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

cfg_rt! {
    /// An owned permission to join on a task (await its termination).
    ///
    /// This can be thought of as the equivalent of [`std::thread::JoinHandle`]
    /// for a Tokio task rather than a thread. You do not need to `.await` the
    /// `JoinHandle` to make the task execute — it will start running in the
    /// background immediately.
    ///
    /// A `JoinHandle` *detaches* the associated task when it is dropped, which
    /// means that there is no longer any handle to the task, and no way to `join`
    /// on it.
    ///
    /// This `struct` is created by the [`task::spawn`] and [`task::spawn_blocking`]
    /// functions.
    ///
    /// # Cancel safety
    ///
    /// The `&mut JoinHandle<T>` type is cancel safe. If it is used as the event
    /// in a `tokio::select!` statement and some other branch completes first,
    /// then it is guaranteed that the output of the task is not lost.
    ///
    /// If a `JoinHandle` is dropped, then the task continues running in the
    /// background and its return value is lost.
    ///
    /// # Examples
    ///
    /// Creation from [`task::spawn`]:
    ///
    /// ```
    /// use tokio::task;
    ///
    /// # async fn doc() {
    /// let join_handle: task::JoinHandle<_> = task::spawn(async {
    ///     // some work here
    /// });
    /// # }
    /// ```
    ///
    /// Creation from [`task::spawn_blocking`]:
    ///
    /// ```
    /// use tokio::task;
    ///
    /// # async fn doc() {
    /// let join_handle: task::JoinHandle<_> = task::spawn_blocking(|| {
    ///     // some blocking work here
    /// });
    /// # }
    /// ```
    ///
    /// The generic parameter `T` in `JoinHandle<T>` is the return type of the spawned task.
    /// If the return value is an i32, the join handle has type `JoinHandle<i32>`:
    ///
    /// ```
    /// use tokio::task;
    ///
    /// # async fn doc() {
    /// let join_handle: task::JoinHandle<i32> = task::spawn(async {
    ///     5 + 3
    /// });
    /// # }
    ///
    /// ```
    ///
    /// If the task does not have a return value, the join handle has type `JoinHandle<()>`:
    ///
    /// ```
    /// use tokio::task;
    ///
    /// # async fn doc() {
    /// let join_handle: task::JoinHandle<()> = task::spawn(async {
    ///     println!("I return nothing.");
    /// });
    /// # }
    /// ```
    ///
    /// Note that `handle.await` doesn't give you the return type directly. It is wrapped in a
    /// `Result` because panics in the spawned task are caught by Tokio. The `?` operator has
    /// to be double chained to extract the returned value:
    ///
    /// ```
    /// use tokio::task;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let join_handle: task::JoinHandle<Result<i32, io::Error>> = tokio::spawn(async {
    ///         Ok(5 + 3)
    ///     });
    ///
    ///     let result = join_handle.await??;
    ///     assert_eq!(result, 8);
    ///     Ok(())
    /// }
    /// ```
    ///
    /// If the task panics, the error is a [`JoinError`] that contains the panic:
    ///
    /// ```
    /// use tokio::task;
    /// use std::io;
    /// use std::panic;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let join_handle: task::JoinHandle<Result<i32, io::Error>> = tokio::spawn(async {
    ///         panic!("boom");
    ///     });
    ///
    ///     let err = join_handle.await.unwrap_err();
    ///     assert!(err.is_panic());
    ///     Ok(())
    /// }
    ///
    /// ```
    /// Child being detached and outliving its parent:
    ///
    /// ```no_run
    /// use tokio::task;
    /// use tokio::time;
    /// use std::time::Duration;
    ///
    /// # #[tokio::main] async fn main() {
    /// let original_task = task::spawn(async {
    ///     let _detached_task = task::spawn(async {
    ///         // Here we sleep to make sure that the first task returns before.
    ///         time::sleep(Duration::from_millis(10)).await;
    ///         // This will be called, even though the JoinHandle is dropped.
    ///         println!("♫ Still alive ♫");
    ///     });
    /// });
    ///
    /// original_task.await.expect("The task being joined has panicked");
    /// println!("Original task is joined.");
    ///
    /// // We make sure that the new task has time to run, before the main
    /// // task returns.
    ///
    /// time::sleep(Duration::from_millis(1000)).await;
    /// # }
    /// ```
    ///
    /// [`task::spawn`]: crate::task::spawn()
    /// [`task::spawn_blocking`]: crate::task::spawn_blocking
    /// [`std::thread::JoinHandle`]: std::thread::JoinHandle
    /// [`JoinError`]: crate::task::JoinError
    pub struct JoinHandle<T> {
        raw: Option<RawTask>,
        id: Id,
        _p: PhantomData<T>,
    }
}

unsafe impl<T: Send> Send for JoinHandle<T> {}
unsafe impl<T: Send> Sync for JoinHandle<T> {}

impl<T> UnwindSafe for JoinHandle<T> {}
impl<T> RefUnwindSafe for JoinHandle<T> {}

impl<T> JoinHandle<T> {
    pub(super) fn new(raw: RawTask, id: Id) -> JoinHandle<T> {
        JoinHandle {
            raw: Some(raw),
            id,
            _p: PhantomData,
        }
    }

    /// Abort the task associated with the handle.
    ///
    /// Awaiting a cancelled task might complete as usual if the task was
    /// already completed at the time it was cancelled, but most likely it
    /// will fail with a [cancelled] `JoinError`.
    ///
    /// ```rust
    /// use tokio::time;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///    let mut handles = Vec::new();
    ///
    ///    handles.push(tokio::spawn(async {
    ///       time::sleep(time::Duration::from_secs(10)).await;
    ///       true
    ///    }));
    ///
    ///    handles.push(tokio::spawn(async {
    ///       time::sleep(time::Duration::from_secs(10)).await;
    ///       false
    ///    }));
    ///
    ///    for handle in &handles {
    ///        handle.abort();
    ///    }
    ///
    ///    for handle in handles {
    ///        assert!(handle.await.unwrap_err().is_cancelled());
    ///    }
    /// }
    /// ```
    /// [cancelled]: method@super::error::JoinError::is_cancelled
    pub fn abort(&self) {
        if let Some(raw) = self.raw {
            raw.remote_abort();
        }
    }

    /// Checks if the task associated with this `JoinHandle` has finished.
    ///
    /// Please note that this method can return `false` even if [`abort`] has been
    /// called on the task. This is because the cancellation process may take
    /// some time, and this method does not return `true` until it has
    /// completed.
    ///
    /// ```rust
    /// use tokio::time;
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// # time::pause();
    /// let handle1 = tokio::spawn(async {
    ///     // do some stuff here
    /// });
    /// let handle2 = tokio::spawn(async {
    ///     // do some other stuff here
    ///     time::sleep(time::Duration::from_secs(10)).await;
    /// });
    /// // Wait for the task to finish
    /// handle2.abort();
    /// time::sleep(time::Duration::from_secs(1)).await;
    /// assert!(handle1.is_finished());
    /// assert!(handle2.is_finished());
    /// # }
    /// ```
    /// [`abort`]: method@JoinHandle::abort
    pub fn is_finished(&self) -> bool {
        if let Some(raw) = self.raw {
            let state = raw.header().state.load();
            state.is_complete()
        } else {
            true
        }
    }

    /// Set the waker that is notified when the task completes.
    pub(crate) fn set_join_waker(&mut self, waker: &Waker) {
        if let Some(raw) = self.raw {
            if raw.try_set_join_waker(waker) {
                // In this case the task has already completed. We wake the waker immediately.
                waker.wake_by_ref();
            }
        }
    }

    /// Returns a new `AbortHandle` that can be used to remotely abort this task.
    pub(crate) fn abort_handle(&self) -> super::AbortHandle {
        let raw = self.raw.map(|raw| {
            raw.ref_inc();
            raw
        });
        super::AbortHandle::new(raw, self.id.clone())
    }

    /// Returns a [task ID] that uniquely identifies this task relative to other
    /// currently spawned tasks.
    ///
    /// **Note**: This is an [unstable API][unstable]. The public API of this type
    /// may break in 1.x releases. See [the documentation on unstable
    /// features][unstable] for details.
    ///
    /// [task ID]: crate::task::Id
    /// [unstable]: crate#unstable-features
    #[cfg(tokio_unstable)]
    #[cfg_attr(docsrs, doc(cfg(tokio_unstable)))]
    pub fn id(&self) -> super::Id {
        self.id.clone()
    }
}

impl<T> Unpin for JoinHandle<T> {}

impl<T> Future for JoinHandle<T> {
    type Output = super::Result<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut ret = Poll::Pending;

        // Keep track of task budget
        let coop = ready!(crate::runtime::coop::poll_proceed(cx));

        // Raw should always be set. If it is not, this is due to polling after
        // completion
        let raw = self
            .raw
            .as_ref()
            .expect("polling after `JoinHandle` already completed");

        // Try to read the task output. If the task is not yet complete, the
        // waker is stored and is notified once the task does complete.
        //
        // The function must go via the vtable, which requires erasing generic
        // types. To do this, the function "return" is placed on the stack
        // **before** calling the function and is passed into the function using
        // `*mut ()`.
        //
        // Safety:
        //
        // The type of `T` must match the task's output type.
        unsafe {
            raw.try_read_output(&mut ret as *mut _ as *mut (), cx.waker());
        }

        if ret.is_ready() {
            coop.made_progress();
        }

        ret
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        if let Some(raw) = self.raw.take() {
            if raw.header().state.drop_join_handle_fast().is_ok() {
                return;
            }

            raw.drop_join_handle_slow();
        }
    }
}

impl<T> fmt::Debug for JoinHandle<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("JoinHandle")
            .field("id", &self.id)
            .finish()
    }
}
