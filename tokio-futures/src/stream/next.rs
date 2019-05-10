use futures::Stream;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future of the next element of a stream.
#[derive(Debug)]
pub struct Next<'a, T> {
    stream: &'a mut T,
}

impl<'a, T: Stream + Unpin> Unpin for Next<'a, T> {}

impl<'a, T: Stream + Unpin> Next<'a, T> {
    pub(super) fn new(stream: &'a mut T) -> Next<'a, T> {
        Next { stream }
    }
}

impl<'a, T: Stream + Unpin> Future for Next<'a, T> {
    type Output = Option<Result<T::Item, T::Error>>;

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        use crate::compat::forward::convert_poll_stream;

        convert_poll_stream(self.stream.poll())
    }
}
