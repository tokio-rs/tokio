use BufStream;
use bytes::Buf;
use futures::{Stream, Poll};

/// TODO: Dox
pub fn stream<T>(stream: T) -> FromStream<T>
where
    T: Stream,
    T::Item: Buf,
{
    FromStream { stream }
}

/// TODO: Dox
#[derive(Debug)]
pub struct FromStream<T> {
    stream: T,
}

impl<T> BufStream for FromStream<T>
where
    T: Stream,
    T::Item: Buf,
{
    type Item = T::Item;
    type Error = T::Error;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.stream.poll()
    }
}
