use crate::BufStream;
use bytes::Buf;
use std::error::Error;
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Converts an `Iterator` into a `BufStream` which is always ready to yield the
/// next value.
///
/// Iterators in Rust don't express the ability to block, so this adapter
/// simply always calls `iter.next()` and returns that.
pub fn iter<I>(i: I) -> Iter<I::IntoIter>
where
    I: IntoIterator,
    I::Item: Buf,
{
    Iter {
        iter: i.into_iter(),
    }
}

/// `BufStream` returned by the [`iter`] function.
#[derive(Debug)]
pub struct Iter<I> {
    iter: I,
}

#[derive(Debug)]
pub enum Never {}

impl<I> BufStream for Iter<I>
where
    I: Iterator,
    I::Item: Buf,
{
    type Item = I::Item;
    type Error = Never;

    fn poll_buf(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>, Self::Error> {
        Ok(self.iter.next()).into()
    }
}

impl fmt::Display for Never {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!();
    }
}

impl Error for Never {
    fn description(&self) -> &str {
        unreachable!();
    }
}
