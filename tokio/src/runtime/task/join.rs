use crate::runtime::task::RawTask;

use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

doc_rt_core! {
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
        inner: variant::Inner<T>
    }
}

unsafe impl<T: Send> Send for JoinHandle<T> {}
unsafe impl<T: Send> Sync for JoinHandle<T> {}

impl<T> JoinHandle<T> {
    pub(super) fn new(raw: RawTask) -> JoinHandle<T> {
        let inner = variant::Inner::new(raw);
        JoinHandle { inner }
    }
}

#[cfg(all(feature = "test-util", tokio_unstable))]
impl<T> JoinHandle<T> {
    pub(crate) fn wrap<F>(
        future: F,
    ) -> (
        Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
        JoinHandle<F::Output>,
    )
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (wrapped, inner) = variant::Inner::<T>::wrap(future);
        (wrapped, JoinHandle { inner })
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = super::Result<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.inner).poll(cx)
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

struct RawTaskJoinHandle<T> {
    raw: Option<RawTask>,
    _p: PhantomData<T>,
}

impl<T> RawTaskJoinHandle<T> {
    pub(super) fn new(raw: RawTask) -> RawTaskJoinHandle<T> {
        RawTaskJoinHandle {
            raw: Some(raw),
            _p: PhantomData,
        }
    }
}

impl<T> Future for RawTaskJoinHandle<T> {
    type Output = super::Result<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut ret = Poll::Pending;

        // Keep track of task budget
        ready!(crate::coop::poll_proceed(cx));

        // Raw should always be set. If it is not, this is due to polling after
        // completion
        let raw = self
            .get_mut()
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

        ret
    }
}

impl<T> Unpin for RawTaskJoinHandle<T> {}

impl<T> Drop for RawTaskJoinHandle<T> {
    fn drop(&mut self) {
        if let Some(raw) = self.raw.take() {
            if raw.header().state.drop_join_handle_fast().is_ok() {
                return;
            }
            raw.drop_join_handle_slow();
        }
    }
}

#[cfg(not(all(feature = "test-util", tokio_unstable)))]
mod variant {
    pub(super) type Inner<T> = super::RawTaskJoinHandle<T>;
}

#[cfg(all(feature = "test-util", tokio_unstable))]
mod variant {
    use super::RawTask;
    use crate::sync::oneshot;
    use std::future::Future;
    use std::panic;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::thread;

    /// Inner type for JoinHandle.
    pub(super) enum Inner<T> {
        RawTask(super::RawTaskJoinHandle<T>),
        Oneshot {
            oneshot: oneshot::Receiver<thread::Result<T>>,
        },
    }

    impl<T> Inner<T> {
        pub(super) fn wrap<F>(
            future: F,
        ) -> (
            Pin<Box<dyn Future<Output = ()> + 'static + Send>>,
            Inner<F::Output>,
        )
        where
            F: Future + Send + 'static,
            F::Output: Send + 'static,
        {
            let (tx, rx) = oneshot::channel();
            let handle = Inner::Oneshot { oneshot: rx };
            let future = Box::pin(future);
            let wrapped = Wrapped {
                inner: future,
                oneshot: Some(tx),
            };
            (Box::pin(wrapped), handle)
        }

        pub(super) fn new(raw: RawTask) -> Inner<T> {
            Inner::RawTask(super::RawTaskJoinHandle::new(raw))
        }
    }

    impl<T> Future for Inner<T> {
        type Output = super::super::Result<T>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            unsafe {
                match self.get_unchecked_mut() {
                    Inner::Oneshot { ref mut oneshot } => {
                        match Pin::new_unchecked(oneshot).poll(cx) {
                            Poll::Pending => Poll::Pending,
                            Poll::Ready(Ok(Ok(result))) => Poll::Ready(Ok(result)),
                            Poll::Ready(Ok(Err(p))) => {
                                Poll::Ready(Err(super::super::JoinError::panic2(p)))
                            }
                            // just count all channel errors as cancellation
                            Poll::Ready(Err(_)) => {
                                Poll::Ready(Err(super::super::JoinError::cancelled2()))
                            }
                        }
                    }
                    Inner::RawTask(ref mut inner) => Pin::new(inner).poll(cx),
                }
            }
        }
    }

    impl<T> Unpin for Inner<T> {}

    /// Wrapped will wrap the Future F in a future which will
    /// poll the inner future, propagating the results via a oneshot channel
    /// to the associated Inner.
    struct Wrapped<F>
    where
        F: Future + Unpin,
    {
        inner: F,
        oneshot: Option<oneshot::Sender<thread::Result<F::Output>>>,
    }

    impl<F> Future for Wrapped<F>
    where
        F: Future + Unpin,
    {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let inner = Pin::new(&mut self.inner);
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| inner.poll(cx)));
            match result {
                Ok(Poll::Pending) => Poll::Pending,
                Ok(Poll::Ready(r)) => {
                    drop(self.oneshot.take().map(|s| s.send(Ok(r))));
                    Poll::Ready(())
                }
                // If a panic is encountered, the inner future is completed
                // immediately return.
                Err(p) => {
                    drop(self.oneshot.take().map(|s| s.send(Err(p))));
                    Poll::Ready(())
                }
            }
        }
    }
}
