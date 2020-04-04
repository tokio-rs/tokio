use crate::stream::Stream;

use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};

/// Stream for the [`empty`] function.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Empty<T>(PhantomData<T>);

impl<T> Unpin for Empty<T> {}
unsafe impl<T> Send for Empty<T> {}
unsafe impl<T> Sync for Empty<T> {}

/// Creates a stream that yields nothing.
///
/// The returned stream is immediately ready and returns `None`. Use
/// [`stream::pending()`](super::pending()) to obtain a stream that is never
/// ready.
///
/// # Examples
///
/// Basic usage:
///
/// ```
/// use tokio::stream::{self, StreamExt};
///
/// #[tokio::main]
/// async fn main() {
///     let mut none = stream::empty::<i32>();
///
///     assert_eq!(None, none.next().await);
/// }
/// ```
pub const fn empty<T>() -> Empty<T> {
    Empty(PhantomData)
}

impl<T> Stream for Empty<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<T>> {
        Poll::Ready(None)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }
}
