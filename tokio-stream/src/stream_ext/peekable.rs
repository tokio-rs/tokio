use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use pin_project_lite::pin_project;

use crate::stream_ext::Fuse;
use crate::StreamExt;

pin_project! {
    /// Stream returned by the [`peekable`](super::StreamExt::peekable) method.
    pub struct Peekable<T: Stream> {
        peek: Option<T::Item>,
        #[pin]
        stream: Fuse<T>,
    }
}

impl<T: Stream> Peekable<T> {
    pub(crate) fn new(stream: T) -> Self {
        let stream = stream.fuse();
        Self { peek: None, stream }
    }

    /// Peek at the next item in the stream.
    pub async fn peek(&mut self) -> Option<&T::Item>
    where
        T: Unpin,
    {
        if let Some(ref it) = self.peek {
            Some(it)
        } else {
            self.peek = self.next().await;
            self.peek.as_ref()
        }
    }
}

impl<T: Stream> Stream for Peekable<T> {
    type Item = T::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        if let Some(it) = this.peek.take() {
            Poll::Ready(Some(it))
        } else {
            this.stream.poll_next(cx)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let peek_len = if self.peek.is_some() { 1 } else { 0 };
        let (lo, hi) = self.stream.size_hint();
        let lo = lo.saturating_add(peek_len);
        let hi = match hi {
            Some(x) => x.checked_add(peek_len),
            None => None,
        };
        (lo, hi)
    }
}
