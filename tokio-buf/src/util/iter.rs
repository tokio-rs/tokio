use BufStream;
use bytes::Buf;
use futures::Poll;
use std::marker::PhantomData;

/// TODO: Dox
pub fn iter<I, E>(i: I) -> Iter<I::IntoIter, E>
where
    I: IntoIterator,
    I::Item: Buf,
{
    Iter {
        iter: i.into_iter(),
        _p: PhantomData,
    }
}

/// TODO: Dox
#[derive(Debug)]
pub struct Iter<I, E> {
    iter: I,
    _p: PhantomData<E>,
}

impl<I, E> BufStream for Iter<I, E>
where
    I: Iterator,
    I::Item: Buf,
{
    type Item = I::Item;
    type Error = E;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        Ok(self.iter.next().into())
    }
}
