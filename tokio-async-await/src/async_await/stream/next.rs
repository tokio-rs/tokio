use futures::Stream;
use futures_core::future::Future;
use futures_core::task::{self, Poll};

use std::marker::Unpin;
use std::pin::PinMut;

/// A future of the next element of a stream.
#[derive(Debug)]
pub struct Next<'a, T: 'a> {
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

    fn poll(self: PinMut<Self>, _cx: &mut task::Context) -> Poll<Self::Output> {
        use crate::async_await::compat::forward::convert_poll_stream;

        convert_poll_stream(
            PinMut::get_mut(self).stream.poll())
    }
}
