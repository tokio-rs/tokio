use UnixDatagram;

use futures::{Async, Future, Poll};

use std::io;
use std::mem;

/// A future for receiving datagrams from the previously connected peer through a Unix datagram socket.
#[derive(Debug)]
pub struct RecvDgram2<T> {
    st: State<T>,
}

#[derive(Debug)]
enum State<T> {
    Receiving { sock: UnixDatagram, buf: T },
    Empty,
}

impl<T> RecvDgram2<T>
where
    T: AsMut<[u8]>,
{
    pub(crate) fn new(sock: UnixDatagram, buf: T) -> RecvDgram2<T> {
        RecvDgram2 {
            st: State::Receiving { sock, buf },
        }
    }
}

impl<T> Future for RecvDgram2<T>
where
    T: AsMut<[u8]>,
{
    /// RecvDgram yields a tuple of the underlying socket, the receive buffer, how many bytes were
    /// received, and the address (path) of the peer sending the datagram. If the buffer is too small, the
    /// datagram is truncated.
    type Item = (UnixDatagram, T, usize);
    /// This future yields io::Error if an error occurred.
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let received;

        if let State::Receiving {
            ref mut sock,
            ref mut buf,
        } = self.st
        {
            let n = try_ready!(sock.poll_recv(buf.as_mut()));
            received = n;
        } else {
            panic!()
        }

        if let State::Receiving { sock, buf } = mem::replace(&mut self.st, State::Empty) {
            Ok(Async::Ready((sock, buf, received)))
        } else {
            panic!()
        }
    }
}
