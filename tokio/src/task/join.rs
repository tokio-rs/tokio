use crate::loom::alloc::Track;
use crate::task::RawTask;

use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// An owned permission to join on a task (await its termination).
///
/// This can be thought of as the equivalent of [`std::thread::JoinHandle`] for
/// a task rather than a thread.
///
/// A `JoinHandle` *detaches* the associated task when it is dropped, which
/// means that there is no longer any handle to the task, and no way to `join`
/// on it.
///
/// This `struct` is created by the [`task::spawn`] and [`task::spawn_blocking`]
/// functions.
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
///         time::delay_for(Duration::from_millis(10)).await;
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
/// time::delay_for(Duration::from_millis(1000)).await;
/// # }
/// ```
///
/// [`task::spawn`]: crate::task::spawn()
/// [`task::spawn_blocking`]: crate::task::spawn_blocking
/// [`std::thread::JoinHandle`]: std::thread::JoinHandle
pub struct JoinHandle<T> {
    raw: Option<RawTask>,
    _p: PhantomData<T>,
}

unsafe impl<T: Send> Send for JoinHandle<T> {}
unsafe impl<T: Send> Sync for JoinHandle<T> {}

impl<T> JoinHandle<T> {
    pub(super) fn new(raw: RawTask) -> JoinHandle<T> {
        JoinHandle {
            raw: Some(raw),
            _p: PhantomData,
        }
    }
}

impl<T> Unpin for JoinHandle<T> {}

impl<T> Future for JoinHandle<T> {
    type Output = super::Result<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use std::mem::MaybeUninit;

        // Raw should always be set
        let raw = self.raw.as_ref().unwrap();

        // Load the current task state
        let mut state = raw.header().state.load();

        debug_assert!(state.is_join_interested());

        if state.is_active() {
            state = if state.has_join_waker() {
                raw.swap_join_waker(cx.waker(), state)
            } else {
                raw.store_join_waker(cx.waker())
            };

            if state.is_active() {
                return Poll::Pending;
            }
        }

        let mut out = MaybeUninit::<Track<Self::Output>>::uninit();

        unsafe {
            // This could result in the task being freed.
            raw.read_output(out.as_mut_ptr() as *mut (), state);

            self.raw = None;

            Poll::Ready(out.assume_init().into_inner())
        }
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        if let Some(raw) = self.raw.take() {
            if raw.header().state.drop_join_handle_fast() {
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
        fmt.debug_struct("JoinHandle").finish()
    }
}
