use super::TcpListener;
use super::TcpStream;

use std::io;
use futures::stream::Stream;
use futures::{Poll, Async};

#[cfg(feature = "unstable-futures")]
use futures2;

/// Stream returned by the `TcpListener::incoming` function representing the
/// stream of sockets received from a listener.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct Incoming {
    inner: TcpListener,
}

impl Incoming {
    pub(crate) fn new(listener: TcpListener) -> Incoming {
        Incoming { inner: listener }
    }
}

impl Stream for Incoming {
    type Item = TcpStream;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, io::Error> {
        let (socket, _) = try_ready!(self.inner.poll_accept());
        Ok(Async::Ready(Some(socket)))
    }
}

#[cfg(feature = "unstable-futures")]
impl futures2::Stream for Incoming {
    type Item = TcpStream;
    type Error = io::Error;

    fn poll_next(&mut self, cx: &mut futures2::task::Context)
        -> futures2::Poll<Option<Self::Item>, io::Error>
    {
        Ok(self.inner.poll_accept2(cx)?.map(|(sock, _)| Some(sock)))
    }
}
