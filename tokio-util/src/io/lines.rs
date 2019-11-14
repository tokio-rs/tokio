use crate::stream::Stream;
use tokio::io::{AsyncBufRead, Lines};

use futures_core::ready;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

impl<R> Stream for Lines<R>
where
    R: AsyncBufRead + Unpin,
{
    type Item = io::Result<String>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match ready!(self.poll_next_line(cx))? {
            Some(line) => Poll::Ready(Some(Ok(line))),
            None => Poll::Ready(None),
        }
    }
}

