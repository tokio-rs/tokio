use crate::stream::Stream;

use core::pin::Pin;
use core::task::{Context, Poll};

/// Stream for the [`iter`] function.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Iter<I> {
    iter: I,
}

impl<I> Unpin for Iter<I> {}

/// Converts an `Iterator` into a `Stream` which is always ready
/// to yield the next value.
///
/// Iterators in Rust don't express the ability to block, so this adapter
/// simply always calls `iter.next()` and returns that.
///
/// ```
/// # async fn dox() {
/// use tokio::stream::{self, StreamExt};
///
/// let mut stream = stream::iter(vec![17, 19]);
///
/// assert_eq!(stream.next().await, Some(17));
/// assert_eq!(stream.next().await, Some(19));
/// assert_eq!(stream.next().await, None);
/// # }
/// ```
pub fn iter<I>(i: I) -> Iter<I::IntoIter>
where
    I: IntoIterator,
{
    Iter {
        iter: i.into_iter(),
    }
}

impl<I> Stream for Iter<I>
where
    I: Iterator,
{
    type Item = I::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<I::Item>> {
        ready!(crate::coop::poll_proceed(cx));
        Poll::Ready(self.iter.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
