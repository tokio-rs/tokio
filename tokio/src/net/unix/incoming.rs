use crate::net::unix::{UnixListener, UnixStream};

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Stream of listeners
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Incoming<'a> {
    inner: &'a mut UnixListener,
}

impl Incoming<'_> {
    pub(crate) fn new(listener: &mut UnixListener) -> Incoming<'_> {
        Incoming { inner: listener }
    }

    #[doc(hidden)] // TODO: dox
    pub fn poll_accept(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<UnixStream>> {
        let (socket, _) = ready!(self.inner.poll_accept(cx))?;
        Poll::Ready(Ok(socket))
    }
}

#[cfg(feature = "stream")]
impl futures_core::Stream for Incoming<'_> {
    type Item = io::Result<UnixStream>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (socket, _) = ready!(self.inner.poll_accept(cx))?;
        Poll::Ready(Some(Ok(socket)))
    }
}
