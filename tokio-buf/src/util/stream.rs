use bytes::Buf;
use futures::{Poll, Stream};
use BufStream;

/// Converts a `Stream` of `Buf` types into a `BufStream`.
///
/// While `Stream` and `BufSream` are very similar, they are not identical. The
/// `stream` function returns a `BufStream` that is backed by the provided
/// `Stream` type.
pub fn stream<T>(stream: T) -> FromStream<T>
where
    T: Stream,
    T::Item: Buf,
{
    FromStream { stream }
}

/// `BufStream` returned by the [`stream`] function.
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
