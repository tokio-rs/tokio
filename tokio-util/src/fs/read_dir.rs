use crate::stream::Stream;
use tokio::fs::{DirEntry, ReadDir};

use futures_core::ready;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

impl Stream for ReadDir {
    type Item = io::Result<DirEntry>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(match ready!(self.poll_next_entry(cx)) {
            Ok(Some(entry)) => Some(Ok(entry)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        })
    }
}
