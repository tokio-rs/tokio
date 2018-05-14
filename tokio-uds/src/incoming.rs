use {UnixListener, UnixStream};

use futures::{Stream, Poll};

use std::io;

/// Stream of listeners
#[derive(Debug)]
pub struct Incoming {
    inner: UnixListener,
}

impl Incoming {
    pub(crate) fn new(listener: UnixListener) -> Incoming {
        Incoming { inner: listener }
    }
}

impl Stream for Incoming {
    type Item = UnixStream;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, io::Error> {
        Ok(Some(try_ready!(self.inner.poll_accept()).0).into())
    }
}

