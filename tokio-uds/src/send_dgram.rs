use UnixDatagram;

use futures::{Async, Future, Poll};

use std::io;
use std::mem;
use std::path::Path;

/// A future for writing a buffer to a Unix datagram socket.
#[derive(Debug)]
pub struct SendDgram<T, P> {
    st: State<T, P>,
}

#[derive(Debug)]
enum State<T, P> {
    /// current state is Sending
    Sending {
        /// the underlying socket
        sock: UnixDatagram,
        /// the buffer to send
        buf: T,
        /// the destination
        addr: P,
    },
    /// neutral state
    Empty,
}

impl<T, P> SendDgram<T, P>
where
    T: AsRef<[u8]>,
    P: AsRef<Path>,
{
    pub(crate) fn new(sock: UnixDatagram, buf: T, addr: P) -> SendDgram<T, P> {
        SendDgram {
            st: State::Sending {
                sock,
                buf,
                addr,
            }
        }
    }
}

impl<T, P> Future for SendDgram<T, P>
where
    T: AsRef<[u8]>,
    P: AsRef<Path>,
{
    /// Returns the underlying socket and the buffer that was sent.
    type Item = (UnixDatagram, T);
    /// The error that is returned when sending failed.
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let State::Sending {
            ref mut sock,
            ref buf,
            ref addr,
        } = self.st
        {
            let n = try_ready!(sock.poll_send_to(buf.as_ref(), addr));
            if n < buf.as_ref().len() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Couldn't send whole buffer".to_string(),
                ));
            }
        } else {
            panic!()
        }
        if let State::Sending { sock, buf, addr: _ } =
            mem::replace(&mut self.st, State::Empty)
        {
            Ok(Async::Ready((sock, buf)))
        } else {
            panic!()
        }
    }
}
