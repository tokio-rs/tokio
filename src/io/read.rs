use std::mem;
use std::io::Read;

use futures::{Async, Future, Poll};

enum State<R, T> {
    Pending {
        rd: R,
        buf: T,
    },
    Empty,
}

/// Tries to read some bytes directly into the given `buf` in asynchronous
/// manner, returning a future type.
///
/// The returned future will resolve to both the I/O stream as well as the
/// buffer once the read operation is completed.
pub fn read<R, T>(rd: R, buf: T) -> ReadSome<R, T>
    where R: Read,
          T: AsMut<[u8]>
{
    ReadSome { state: State::Pending { rd: rd, buf: buf } }
}

/// A future which can be used to easily read available number of bytes to fill
/// a buffer.
///
/// Created by the [`read`] function.
pub struct ReadSome<R, T> {
    state: State<R, T>,
}

impl<R, T> Future for ReadSome<R, T>
    where R: Read,
          T: AsMut<[u8]>
{
    type Item = (R, T, usize);
    type Error = ::std::io::Error;

    fn poll(&mut self) -> Poll<(R, T, usize), ::std::io::Error> {
        let nread = match self.state {
            State::Pending { ref mut rd, ref mut buf } => {
                let buf = buf.as_mut();

                match rd.read(&mut buf[..]) {
                    Ok(nread) => nread,
                    Err(ref err) if err.kind() == ::std::io::ErrorKind::WouldBlock => {
                        return Ok(Async::NotReady)
                    }
                    Err(err) => return Err(err.into()),
                }
            }
            State::Empty => panic!("poll a ReadSome after it's done"),
        };

        match mem::replace(&mut self.state, State::Empty) {
            State::Pending { rd, buf } => Ok((rd, buf, nread).into()),
            State::Empty => panic!("invalid internal state"),
        }
    }
}
