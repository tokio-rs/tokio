use crate::stream::Stream;
use tokio::sync::watch::Receiver;

use futures_core::ready;
use std::pin::Pin;
use std::task::{Context, Poll};

impl<T: Clone> Stream for Receiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let ret = ready!(self.poll_recv_ref(cx));

        #[allow(clippy::map_clone)]
        Poll::Ready(ret.map(|v_ref| v_ref.clone()))
    }
}
