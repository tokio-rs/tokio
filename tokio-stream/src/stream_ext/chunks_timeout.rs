use crate::stream_ext::Fuse;
use crate::Stream;
use tokio::time::{sleep, Instant, Sleep};

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::pin_project;
use std::time::Duration;

pin_project! {
    /// Stream returned by the [`chunks_timeout`](super::StreamExt::chunks_timeout) method.
    #[must_use = "streams do nothing unless polled"]
    #[derive(Debug)]
    pub struct ChunksTimeout<S: Stream> {
        #[pin]
        stream: Fuse<S>,
        #[pin]
        deadline: Sleep,
        duration: Duration,
        items: Vec<S::Item>,
        cap: usize, // https://github.com/rust-lang/futures-rs/issues/1475
    }
}

impl<S: Stream> ChunksTimeout<S> {
    pub(super) fn new(stream: S, max_size: usize, duration: Duration) -> Self {
        ChunksTimeout {
            stream: Fuse::new(stream),
            deadline: sleep(duration),
            duration,
            items: Vec::with_capacity(max_size),
            cap: max_size,
        }
    }
}

impl<S: Stream> Stream for ChunksTimeout<S> {
    type Item = Vec<S::Item>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut me = self.as_mut().project();
        loop {
            match me.stream.as_mut().poll_next(cx) {
                Poll::Pending => break,
                Poll::Ready(Some(item)) => {
                    if me.items.is_empty() {
                        me.deadline.as_mut().reset(Instant::now() + *me.duration);
                        me.items.reserve_exact(*me.cap);
                    }
                    me.items.push(item);
                    if me.items.len() >= *me.cap {
                        return Poll::Ready(Some(std::mem::take(me.items)));
                    }
                }
                Poll::Ready(None) => {
                    // Returning Some here is only correct because we fuse the inner stream.
                    let last = if me.items.is_empty() {
                        None
                    } else {
                        Some(std::mem::take(me.items))
                    };

                    return Poll::Ready(last);
                }
            }
        }

        if !me.items.is_empty() {
            ready!(me.deadline.poll(cx));
            return Poll::Ready(Some(std::mem::take(me.items)));
        }

        Poll::Pending
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let chunk_len = if self.items.is_empty() { 0 } else { 1 };
        let (lower, upper) = self.stream.size_hint();
        let lower = (lower / self.cap).saturating_add(chunk_len);
        let upper = upper.and_then(|x| x.checked_add(chunk_len));
        (lower, upper)
    }
}
