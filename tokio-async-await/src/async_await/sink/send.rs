use futures::Sink;

use futures_core::future::Future;
use futures_core::task::{self, Poll};

use std::marker::Unpin;
use std::mem::PinMut;

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

    fn poll(mut self: PinMut<Self>, _cx: &mut task::Context) -> Poll<Self::Output> {
        use crate::async_await::compat::forward::convert_poll;
        use futures::AsyncSink::{Ready, NotReady};
        use futures_util::try_ready;

        // use crate::compat::forward::convert_poll;

        let this = &mut *self;

        if let Some(item) = this.item.take() {
            match this.sink.start_send(item) {
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
        try_ready!(convert_poll(this.sink.poll_complete()));

        Poll::Ready(Ok(()))
    }
}
