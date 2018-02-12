use std::io;
use std::mem;

use futures::{Future, Poll};

use AsyncRead;

#[derive(Debug)]
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
/// The returned future will resolve to both the I/O stream and the buffer
/// as well as the number of bytes read once the read operation is completed.
pub fn read<R, T>(rd: R, buf: T) -> Read<R, T>
    where R: AsyncRead,
          T: AsMut<[u8]>
{
    Read { state: State::Pending { rd: rd, buf: buf } }
}

/// A future which can be used to easily read available number of bytes to fill
/// a buffer.
///
/// Created by the [`read`] function.
#[derive(Debug)]
pub struct Read<R, T> {
    state: State<R, T>,
}

impl<R, T> Future for Read<R, T>
    where R: AsyncRead,
          T: AsMut<[u8]>
{
    type Item = (R, T, usize);
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(R, T, usize), io::Error> {
        let nread = match self.state {
            State::Pending { ref mut rd, ref mut buf } => try_nb!(rd.read(&mut buf.as_mut()[..])),
            State::Empty => panic!("poll a Read after it's done"),
        };

        match mem::replace(&mut self.state, State::Empty) {
            State::Pending { rd, buf } => Ok((rd, buf, nread).into()),
            State::Empty => panic!("invalid internal state"),
        }
    }
}
