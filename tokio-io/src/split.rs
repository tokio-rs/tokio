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
        let mut l = try_ready!(wrap_as_io(self.handle.poll_lock()));
        l.read_buf(buf)
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
        let mut l = try_ready!(wrap_as_io(self.handle.poll_lock()));
        l.shutdown()
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error>
        where Self: Sized,
    {
        let mut l = try_ready!(wrap_as_io(self.handle.poll_lock()));
        l.write_buf(buf)
    }
}

fn wrap_as_io<T>(t: Async<T>) -> Result<Async<T>, io::Error> {
    Ok(t)
}

#[cfg(test)]
mod tests {
    extern crate tokio_current_thread;

    use super::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};
    use bytes::{BytesMut, IntoBuf};
    use futures::{Async, Poll, future::lazy, future::ok};
    use futures::sync::BiLock;

    use std::io::{self, Read, Write};

    struct RW;

    impl Read for RW {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Ok(1)
        }
    }

    impl AsyncRead for RW {}

    impl Write for RW {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Ok(1)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl AsyncWrite for RW {
        fn shutdown(&mut self) -> Poll<(), io::Error> {
            Ok(Async::Ready(()))
        }
    }

    #[test]
    fn split_readhalf_translate_wouldblock_to_not_ready() {
        tokio_current_thread::block_on_all(lazy(move || {
            let rw = RW {};
            let (a, b) = BiLock::new(rw);
            let mut rx = ReadHalf { handle: a };

            let mut buf = BytesMut::with_capacity(64);

            // First read is uncontended, should go through.
            assert!(rx.read_buf(&mut buf).unwrap().is_ready());

            // Take lock from write side.
            let lock = b.poll_lock();

            // Second read should be NotReady.
            assert!(!rx.read_buf(&mut buf).unwrap().is_ready());

            drop(lock);

            // Back to uncontended.
            assert!(rx.read_buf(&mut buf).unwrap().is_ready());

            ok::<(), ()>(())
        })).unwrap();
    }

    #[test]
    fn split_writehalf_translate_wouldblock_to_not_ready() {
        tokio_current_thread::block_on_all(lazy(move || {
            let rw = RW {};
            let (a, b) = BiLock::new(rw);
            let mut tx = WriteHalf { handle: a };

            let bufmut = BytesMut::with_capacity(64);
            let mut buf = bufmut.into_buf();

            // First write is uncontended, should go through.
            assert!(tx.write_buf(&mut buf).unwrap().is_ready());

            // Take lock from read side.
            let lock = b.poll_lock();

            // Second write should be NotReady.
            assert!(!tx.write_buf(&mut buf).unwrap().is_ready());

            drop(lock);

            // Back to uncontended.
            assert!(tx.write_buf(&mut buf).unwrap().is_ready());

            ok::<(), ()>(())
        })).unwrap();
    }
}
