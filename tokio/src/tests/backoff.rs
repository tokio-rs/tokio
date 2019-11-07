use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub(crate) struct Backoff(usize, bool);

pub(crate) fn backoff(n: usize) -> impl Future<Output = ()> {
    Backoff(n, false)
}

/// Back off, but clone the waker each time
pub(crate) fn backoff_clone(n: usize) -> impl Future<Output = ()> {
    Backoff(n, true)
}

impl Future for Backoff {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0 == 0 {
            return Poll::Ready(());
        }

        self.0 -= 1;
        if self.1 {
            cx.waker().clone().wake();
        } else {
            cx.waker().wake_by_ref();
        }
        Poll::Pending
    }
}
