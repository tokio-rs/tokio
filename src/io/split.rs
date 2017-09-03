use std::io::{self, Read, Write};

use futures::Async;
use futures::sync::BiLock;

use io::Io;

/// The readable half of an object returned from `Io::split`.
pub struct ReadHalf<T> {
    handle: BiLock<T>,
}

/// The writable half of an object returned from `Io::split`.
pub struct WriteHalf<T> {
    handle: BiLock<T>,
}

pub fn split<T: Io>(t: T) -> (ReadHalf<T>, WriteHalf<T>) {
    let (a, b) = BiLock::new(t);
    (ReadHalf { handle: a }, WriteHalf { handle: b })
}

impl<T: Io> ReadHalf<T> {
    /// Calls the underlying `poll_read` function on this handling, testing to
    /// see if it's ready to be read from.
    pub fn poll_read(&mut self) -> Async<()> {
        match self.handle.poll_lock() {
            Async::Ready(mut l) => l.poll_read(),
            Async::NotReady => Async::NotReady,
        }
    }
}

impl<T: Io> WriteHalf<T> {
    /// Calls the underlying `poll_write` function on this handling, testing to
    /// see if it's ready to be written to.
    pub fn poll_write(&mut self) -> Async<()> {
        match self.handle.poll_lock() {
            Async::Ready(mut l) => l.poll_write(),
            Async::NotReady => Async::NotReady,
        }
    }
}

impl<T: Read> Read for ReadHalf<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.handle.poll_lock() {
            Async::Ready(mut l) => l.read(buf),
            Async::NotReady => Err(io::ErrorKind::WouldBlock.into()),
        }
    }
}

impl<T: Write> Write for WriteHalf<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.handle.poll_lock() {
            Async::Ready(mut l) => l.write(buf),
            Async::NotReady => Err(io::ErrorKind::WouldBlock.into()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.handle.poll_lock() {
            Async::Ready(mut l) => l.flush(),
            Async::NotReady => Err(io::ErrorKind::WouldBlock.into()),
        }
    }
}
