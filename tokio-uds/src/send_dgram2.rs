use UnixDatagram;

use futures::{Async, Future, Poll};

use std::io;
use std::mem;

/// A future for sending datagrams to a previously connected peer through a Unix datagram socket.
#[derive(Debug)]
pub struct SendDgram2<T> {
    st: State<T>,
}

#[derive(Debug)]
enum State<T> {
    /// current state is Sending
    Sending {
        /// the underlying socket
        sock: UnixDatagram,
        /// the buffer to send
        buf: T,
    },
    /// neutral state
    Empty,
}

impl<T> SendDgram2<T>
where
    T: AsRef<[u8]>
{
    pub(crate) fn new(sock: UnixDatagram, buf: T) -> SendDgram2<T> {
        SendDgram2 {
            st: State::Sending { sock, buf },
        }
    }
}

impl<T> Future for SendDgram2<T>
where
    T: AsRef<[u8]>,
{
    /// Returns the underlying socket and the buffer that was sent.
    type Item = (UnixDatagram, T);
    /// The error that is returned when sending failed.
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let State::Sending {
            ref mut sock,
            ref buf,
        } = self.st
        {
            let n = try_ready!(sock.poll_send(buf.as_ref()));
            if n < buf.as_ref().len() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Couldn't send whole buffer".to_string(),
                ));
            }
        } else {
            panic!()
        }
        if let State::Sending { sock, buf } = mem::replace(&mut self.st, State::Empty) {
            Ok(Async::Ready((sock, buf)))
        } else {
            panic!()
        }
    }
}
