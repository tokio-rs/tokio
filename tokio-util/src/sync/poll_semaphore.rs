use futures_core::{ready, Stream};
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{AcquireError, OwnedSemaphorePermit, Semaphore, TryAcquireError};

use super::ReusableBoxFuture;

/// A wrapper around [`Semaphore`] that provides a `poll_acquire` method.
///
/// [`Semaphore`]: tokio::sync::Semaphore
pub struct PollSemaphore {
    semaphore: Arc<Semaphore>,
    permit_fut: Option<ReusableBoxFuture<'static, Result<OwnedSemaphorePermit, AcquireError>>>,
}

impl PollSemaphore {
    /// Create a new `PollSemaphore`.
    pub fn new(semaphore: Arc<Semaphore>) -> Self {
        Self {
            semaphore,
            permit_fut: None,
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
        let permit_future = match self.permit_fut.as_mut() {
            Some(fut) => fut,
            None => {
                // avoid allocations completely if we can grab a permit immediately
                match Arc::clone(&self.semaphore).try_acquire_owned() {
                    Ok(permit) => return Poll::Ready(Some(permit)),
                    Err(TryAcquireError::Closed) => return Poll::Ready(None),
                    Err(TryAcquireError::NoPermits) => {}
                }

                let next_fut = Arc::clone(&self.semaphore).acquire_owned();
                self.permit_fut
                    .get_or_insert(ReusableBoxFuture::new(next_fut))
            }
        };

        let result = ready!(permit_future.poll(cx));

        let next_fut = Arc::clone(&self.semaphore).acquire_owned();
        permit_future.set(next_fut);

        match result {
            Ok(permit) => Poll::Ready(Some(permit)),
            Err(_closed) => {
                self.permit_fut = None;
                Poll::Ready(None)
            }
        }
    }

    /// Returns the current number of available permits.
    ///
    /// This is equivalent to the [`Semaphore::available_permits`] method on the
    /// `tokio::sync::Semaphore` type.
    ///
    /// [`Semaphore::available_permits`]: tokio::sync::Semaphore::available_permits
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Adds `n` new permits to the semaphore.
    ///
    /// The maximum number of permits is `usize::MAX >> 3`, and this function
    /// will panic if the limit is exceeded.
    ///
    /// This is equivalent to the [`Semaphore::add_permits`] method on the
    /// `tokio::sync::Semaphore` type.
    ///
    /// [`Semaphore::add_permits`]: tokio::sync::Semaphore::add_permits
    pub fn add_permits(&self, n: usize) {
        self.semaphore.add_permits(n);
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

impl AsRef<Semaphore> for PollSemaphore {
    fn as_ref(&self) -> &Semaphore {
        &*self.semaphore
    }
}
