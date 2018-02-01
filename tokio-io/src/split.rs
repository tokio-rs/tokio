use std::io::{self, Read, Write};

use futures::{Async, Poll};
use futures::sync::BiLock;
use bytes::{Buf, BufMut};

use {AsyncRead, AsyncWrite};

/// The readable half of an object returned from `AsyncRead::split`.
#[derive(Debug)]
pub struct ReadHalf<T> {
    handle: BiLock<T>,
}

/// The writable half of an object returned from `AsyncRead::split`.
#[derive(Debug)]
pub struct WriteHalf<T> {
    handle: BiLock<T>,
}

pub fn split<T: AsyncRead + AsyncWrite>(t: T) -> (ReadHalf<T>, WriteHalf<T>) {
    let (a, b) = BiLock::new(t);
    (ReadHalf { handle: a }, WriteHalf { handle: b })
}

fn would_block() -> io::Error {
    io::Error::new(io::ErrorKind::WouldBlock, "would block")
}

impl<T: AsyncRead> Read for ReadHalf<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.handle.poll_lock() {
            Async::Ready(mut l) => l.read(buf),
            Async::NotReady => Err(would_block()),
        }
    }
}

impl<T: AsyncRead> AsyncRead for ReadHalf<T> {
    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        match self.handle.poll_lock() {
            Async::Ready(mut l) => l.read_buf(buf),
            Async::NotReady => Err(would_block()),
        }
    }
}

impl<T: AsyncWrite> Write for WriteHalf<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.handle.poll_lock() {
            Async::Ready(mut l) => l.write(buf),
            Async::NotReady => Err(would_block()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.handle.poll_lock() {
            Async::Ready(mut l) => l.flush(),
            Async::NotReady => Err(would_block()),
        }
    }
}

impl<T: AsyncWrite> AsyncWrite for WriteHalf<T> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        match self.handle.poll_lock() {
            Async::Ready(mut l) => l.shutdown(),
            Async::NotReady => Err(would_block()),
        }
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error>
        where Self: Sized,
    {
        match self.handle.poll_lock() {
            Async::Ready(mut l) => l.write_buf(buf),
            Async::NotReady => Err(would_block()),
        }
    }
}
