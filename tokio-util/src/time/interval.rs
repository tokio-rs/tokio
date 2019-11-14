use crate::stream::Stream;
use tokio::time::{Instant, Interval};

use futures_core::ready;
use std::pin::Pin;
use std::task::{Context, Poll};

impl Stream for Interval {
    type Item = Instant;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Instant>> {
        let instant = ready!(self.get_mut().poll_tick(cx));
        Poll::Ready(Some(instant))
    }
}
