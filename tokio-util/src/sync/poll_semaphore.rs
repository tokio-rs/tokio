use futures_core::Stream;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// A wrapper around [`Semaphore`] that provides a `poll_acquire` method.
///
/// [`Semaphore`]: tokio::sync::Semaphore
pub struct PollSemaphore {
    semaphore: Arc<Semaphore>,
    inner: Pin<Box<dyn Stream<Item = OwnedSemaphorePermit> + Send + Sync>>,
}

impl PollSemaphore {
    /// Create a new `PollSemaphore`.
    pub fn new(semaphore: Arc<Semaphore>) -> Self {
        Self {
            semaphore: semaphore.clone(),
            inner: Box::pin(async_stream::stream! {
                loop {
                    match semaphore.clone().acquire_owned().await {
                        Ok(permit) => yield permit,
                        Err(_closed) => break,
                    }
                }
            }),
        }
    }

    /// Closes the semaphore.
    pub fn close(&self) {
        self.semaphore.close()
    }

    /// Obtain a clone of the inner semaphore.
    pub fn clone_inner(&self) -> Arc<Semaphore> {
        self.semaphore.clone()
    }

    /// Get back the inner semaphore.
    pub fn into_inner(self) -> Arc<Semaphore> {
        self.semaphore
    }

    /// Poll to acquire a permit from the semaphore.
    ///
    /// This can return the following values:
    ///
    ///  - `Poll::Pending` if a permit is not currently available.
    ///  - `Poll::Ready(Some(permit))` if a permit was acquired.
    ///  - `Poll::Ready(None)` if the semaphore has been closed.
    ///
    /// When this method returns `Poll::Pending`, the current task is scheduled
    /// to receive a wakeup when a permit becomes available, or when the
    /// semaphore is closed. Note that on multiple calls to `poll_acquire`, only
    /// the `Waker` from the `Context` passed to the most recent call is
    /// scheduled to receive a wakeup.
    pub fn poll_acquire(&mut self, cx: &mut Context<'_>) -> Poll<Option<OwnedSemaphorePermit>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl Stream for PollSemaphore {
    type Item = OwnedSemaphorePermit;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<OwnedSemaphorePermit>> {
        Pin::into_inner(self).poll_acquire(cx)
    }
}

impl Clone for PollSemaphore {
    fn clone(&self) -> PollSemaphore {
        PollSemaphore::new(self.clone_inner())
    }
}

impl fmt::Debug for PollSemaphore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PollSemaphore")
            .field("semaphore", &self.semaphore)
            .finish()
    }
}
