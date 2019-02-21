use futures::Sink;

use std::future::Future;
use std::task::{self, Poll};

use std::marker::Unpin;
use std::pin::Pin;

/// Future for the `SinkExt::send_async` combinator, which sends a value to a
/// sink and then waits until the sink has fully flushed.
#[derive(Debug)]
pub struct Send<'a, T: Sink + 'a + ?Sized> {
    sink: &'a mut T,
    item: Option<T::SinkItem>,
}

impl<T: Sink + Unpin + ?Sized> Unpin for Send<'_, T> {}

impl<'a, T: Sink + Unpin + ?Sized> Send<'a, T> {
    pub(super) fn new(sink: &'a mut T, item: T::SinkItem) -> Self {
        Send {
            sink,
            item: Some(item),
        }
    }
}

impl<T: Sink + Unpin + ?Sized> Future for Send<'_, T> {
    type Output = Result<(), T::SinkError>;

    fn poll(mut self: Pin<&mut Self>, _lw: &task::LocalWaker) -> Poll<Self::Output> {
        use crate::compat::forward::convert_poll;
        use futures::AsyncSink::{NotReady, Ready};

        if let Some(item) = self.item.take() {
            match self.sink.start_send(item) {
                Ok(Ready) => {}
                Ok(NotReady(val)) => {
                    self.item = Some(val);
                    return Poll::Pending;
                }
                Err(err) => {
                    return Poll::Ready(Err(err));
                }
            }
        }

        // we're done sending the item, but want to block on flushing the
        // sink
        try_ready!(convert_poll(self.sink.poll_complete()));

        Poll::Ready(Ok(()))
    }
}
