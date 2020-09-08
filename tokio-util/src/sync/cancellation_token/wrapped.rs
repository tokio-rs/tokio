//! Defines Wrapped future.

use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Error returned when future was aborted
#[derive(Debug)]
pub struct Aborted(());

impl std::fmt::Display for Aborted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        "operation was aborted".fmt(f)
    }
}

impl std::error::Error for Aborted {}
pin_project! {
    /// Future with lifetime is bound to a [`CancellationToken`].
    /// When token is cancelled, on next poll this future will resolve with
    /// Err([`Aborted`]),
    /// and inner future will be dropped.
    /// This future can be constructed using [`wrap_future`] method
    /// on [`CancellationToken`]
    ///
    /// [`CancellationToken`]: crate::sync::CancellationToken
    /// [`Aborted`]: crate::sync::Aborted
    #[derive(Debug)]
    pub struct Wrapped<F> {
        #[pin]
        pub(super) cancelled: super::WaitForCancellationFuture<'static>,
        #[pin]
        pub(super) fut: Option<F>,
    }
}

impl<F> Future for Wrapped<F>
where
    F: Future,
{
    type Output = Result<F::Output, Aborted>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        // if `fut` is None, then operation was cancelled long-long ago,
        // i.e. before previous poll.
        let fut = match this.fut.as_mut().as_pin_mut() {
            Some(fut) => fut,
            None => return Poll::Ready(Err(Aborted(()))),
        };
        if this.cancelled.poll(cx).is_ready() {
            // cancellation was requested.
            this.fut.set(None);
            Poll::Ready(Err(Aborted(())))
        } else {
            // we are still allowed to work.
            fut.poll(cx).map(Ok)
        }
    }
}
