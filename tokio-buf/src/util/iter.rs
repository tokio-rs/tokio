use bytes::Buf;
use futures::Poll;
use std::error::Error;
use std::fmt;
use BufStream;

/// TODO: Dox
pub fn iter<I>(i: I) -> Iter<I::IntoIter>
where
    I: IntoIterator,
    I::Item: Buf,
{
    Iter {
        iter: i.into_iter(),
    }
}

/// TODO: Dox
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

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        Ok(self.iter.next().into())
    }
}

impl fmt::Display for Never {
    fn fmt(&self, _: &mut fmt::Formatter) -> fmt::Result {
        unreachable!();
    }
}

impl Error for Never {
    fn description(&self) -> &str {
        unreachable!();
    }
}
