use crate::stream::Stream;
use tokio::io::{AsyncBufRead, Split};

use futures_core::ready;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

impl<R> Stream for Split<R>
where
    R: AsyncBufRead + Unpin,
{
    type Item = io::Result<Vec<u8>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match ready!(self.poll_next_segment(cx))? {
            Some(segment) => Poll::Ready(Some(Ok(segment))),
            None => Poll::Ready(None),
        }
    }
}
