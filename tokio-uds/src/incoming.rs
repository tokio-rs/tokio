#![cfg(feature = "async-traits")]

use crate::{UnixListener, UnixStream};
use futures_core::stream::Stream;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Stream of listeners
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Incoming {
    inner: UnixListener,
}

impl Incoming {
    pub(crate) fn new(listener: UnixListener) -> Incoming {
        Incoming { inner: listener }
    }
}

impl Stream for Incoming {
    type Item = io::Result<UnixStream>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (socket, _) = ready!(Pin::new(&mut self.inner).poll_accept(cx))?;
        Poll::Ready(Some(Ok(socket)))
    }
}
