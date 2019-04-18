use bytes::Buf;
use futures::{Async, Poll, Stream};
use BufStream;

/// Converts a `Stream` of `Buf` types into a `BufStream`.
///
/// While `Stream` and `BufStream` are very similar, they are not identical. The
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

/// Converts a `BufStream` into a `Stream`.
#[derive(Debug)]
pub struct IntoStream<T> {
    buf: T,
}

impl<T> IntoStream<T> {
    /// Create a new `Stream` from the provided `BufStream`.
    pub fn new(buf: T) -> Self {
        IntoStream { buf }
    }

    /// Get a reference to the inner `BufStream`.
    pub fn get_ref(&self) -> &T {
        &self.buf
    }

    /// Get a mutable reference to the inner `BufStream`
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.buf
    }

    /// Get the inner `BufStream`.
    pub fn into_inner(self) -> T {
        self.buf
    }
}

impl<T: BufStream> Stream for IntoStream<T> {
    type Item = T::Item;
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.buf.poll_buf()? {
            Async::Ready(Some(buf)) => Ok(Async::Ready(Some(buf))),
            Async::Ready(None) => Ok(Async::Ready(None)),
            Async::NotReady => Ok(Async::NotReady),
        }
    }
}
