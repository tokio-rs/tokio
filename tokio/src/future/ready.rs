use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Future for the [`ready`](ready()) function.
///
/// `pub` in order to use the future as an associated type in a sealed trait.
#[derive(Debug)]
// Used as an associated type in a "sealed" trait.
#[allow(unreachable_pub)]
pub struct Ready<T>(Option<T>);

impl<T> Unpin for Ready<T> {}

impl<T> Future for Ready<T> {
    type Output = T;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<T> {
        Poll::Ready(self.0.take().unwrap())
    }
}

/// Creates a future that is immediately ready with a success value.
pub(crate) fn ok<T, E>(t: T) -> Ready<Result<T, E>> {
    Ready(Some(Ok(t)))
}
