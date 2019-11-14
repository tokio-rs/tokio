use crate::stream::Stream;
use tokio::sync::mpsc::{Receiver, UnboundedReceiver};

use std::pin::Pin;
use std::task::{Context, Poll};

impl<T> Stream for Receiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.get_mut().poll_recv(cx)
    }
}

impl<T> Stream for UnboundedReceiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.get_mut().poll_recv(cx)
    }
}
