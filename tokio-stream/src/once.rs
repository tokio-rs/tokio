use crate::Stream;

use core::pin::Pin;
use core::task::{Context, Poll};

/// Stream for the [`once`](fn@once) function.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Once<T> {
    value: Option<T>,
}

impl<I> Unpin for Once<I> {}

/// Creates a stream that emits an element exactly once.
///
/// The returned stream is immediately ready and emits the provided value once.
///
/// # Examples
///
/// ```
/// use tokio_stream::{self as stream, StreamExt};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// // one is the loneliest number
/// let mut one = stream::once(1);
///
/// assert_eq!(Some(1), one.next().await);
///
/// // just one, that's all we get
/// assert_eq!(None, one.next().await);
/// # }
/// ```
pub fn once<T>(value: T) -> Once<T> {
    Once { value: Some(value) }
}

impl<T> Stream for Once<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<T>> {
        #[cfg(feature = "rt")]
        {
            use tokio::task::coop;

            let coop = std::task::ready!(coop::poll_proceed(_cx));

            coop.made_progress();
        }

        Poll::Ready(self.value.take())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.value.is_some() {
            (1, Some(1))
        } else {
            (0, Some(0))
        }
    }
}
