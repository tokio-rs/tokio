use futures_core::{ready, Stream};
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{AcquireError, OwnedSemaphorePermit, Semaphore};

use super::ReusableBoxFuture;

/// A wrapper around [`Semaphore`] that provides a `poll_acquire` method.
///
/// [`Semaphore`]: tokio::sync::Semaphore
pub struct PollSemaphore {
    semaphore: Arc<Semaphore>,
    permit_fut: ReusableBoxFuture<Result<OwnedSemaphorePermit, AcquireError>>,
}

impl PollSemaphore {
    /// Create a new `PollSemaphore`.
    pub fn new(semaphore: Arc<Semaphore>) -> Self {
        let fut = Arc::clone(&semaphore).acquire_owned();

        Self {
            semaphore,
            permit_fut: ReusableBoxFuture::new(fut),
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
        match ready!(self.permit_fut.poll(cx)) {
            Ok(permit) => {
                let next_fut = Arc::clone(&self.semaphore).acquire_owned();
                self.permit_fut.set(next_fut);
                Poll::Ready(Some(permit))
            }
            Err(_closed) => Poll::Ready(None),
        }
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
