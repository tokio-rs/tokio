use UnixDatagram;

use futures::{Async, Future, Poll};

use std::io;
use std::mem;

/// A future for receiving datagrams through a Unix datagram socket.
///
/// An example that uses UDP sockets but is still applicable can be found at
/// https://gist.github.com/dermesser/e331094c2ab28fc7f6ba8a16183fe4d5.
#[derive(Debug)]
pub struct RecvDgramFrom<T> {
    st: State<T>,
}

#[derive(Debug)]
enum State<T> {
    Receiving { sock: UnixDatagram, buf: T },
    Empty,
}

impl<T> RecvDgramFrom<T>
where
    T: AsMut<[u8]>,
{
    pub(crate) fn new(sock: UnixDatagram, buf: T) -> RecvDgramFrom<T> {
        RecvDgramFrom {
            st: State::Receiving { sock, buf },
        }
    }
}

impl<T> Future for RecvDgramFrom<T>
where
    T: AsMut<[u8]>,
{
    /// RecvDgram yields a tuple of the underlying socket, the receive buffer, how many bytes were
    /// received, and the address (path) of the peer sending the datagram. If the buffer is too small, the
    /// datagram is truncated.
    type Item = (UnixDatagram, T, usize, String);
    /// This future yields io::Error if an error occurred.
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let received;
        let peer;

        if let State::Receiving {
            ref mut sock,
            ref mut buf,
        } = self.st
        {
            let (n, p) = try_ready!(sock.poll_recv_from(buf.as_mut()));
            received = n;

            peer = p.as_pathname().map_or(String::new(), |p| {
                p.to_str().map_or(String::new(), |s| s.to_string())
            });
        } else {
            panic!()
        }

        if let State::Receiving { sock, buf } = mem::replace(&mut self.st, State::Empty) {
            Ok(Async::Ready((sock, buf, received, peer)))
        } else {
            panic!()
        }
    }
}
