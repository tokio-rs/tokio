use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project_lite::pin_project;

use crate::sync::{CancellationToken, WaitForCancellationFuture, WaitForCancellationFutureOwned};

pin_project! {
    /// A Future that is resolved once the corresponding [`CancellationToken`]
    /// is cancelled or a given Future gets resolved. It is biased towards the
    /// [`CancellationToken`] cancelled.
    #[must_use = "futures do nothing unless polled"]
    pub struct WithCancellationTokenFuture<'a, F: Future> {
        #[pin]
        cancellation: WaitForCancellationFuture<'a>,
        #[pin]
        future: F,
    }
}

impl<'a, F: Future> WithCancellationTokenFuture<'a, F> {
    pub(crate) fn new(cancellation_token: &'a CancellationToken, future: F) -> Self {
        Self {
            cancellation: cancellation_token.cancelled(),
            future,
        }
    }
}

impl<'a, F: Future> Future for WithCancellationTokenFuture<'a, F> {
    type Output = Option<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if this.cancellation.poll(cx).is_ready() {
            Poll::Ready(None)
        } else if let Poll::Ready(res) = this.future.poll(cx) {
            Poll::Ready(Some(res))
        } else {
            Poll::Pending
        }
    }
}

pin_project! {
    /// A Future that is resolved once the corresponding [`CancellationToken`]
    /// is cancelled or a given Future gets resolved. It is biased towards the
    /// [`CancellationToken`] cancelled.
    #[must_use = "futures do nothing unless polled"]
    pub struct WithCancellationTokenFutureOwned<F: Future> {
        #[pin]
        cancellation: WaitForCancellationFutureOwned,
        #[pin]
        future: F,
    }
}

impl<F: Future> WithCancellationTokenFutureOwned<F> {
    pub(crate) fn new(cancellation_token: CancellationToken, future: F) -> Self {
        Self {
            cancellation: cancellation_token.cancelled_owned(),
            future,
        }
    }
}

impl<F: Future> Future for WithCancellationTokenFutureOwned<F> {
    type Output = Option<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if this.cancellation.poll(cx).is_ready() {
            Poll::Ready(None)
        } else if let Poll::Ready(res) = this.future.poll(cx) {
            Poll::Ready(Some(res))
        } else {
            Poll::Pending
        }
    }
}
