use std::{error, fmt};
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

impl<T: AsyncRead + AsyncWrite> ReadHalf<T> {
    /// Reunite with a previously split `WriteHalf`.
    ///
    /// If this `ReadHalf` and the given `WriteHalf` do not originate from
    /// the same `AsyncRead::split` operation they will be returned, wrapped
    /// in an `UnsplitError`.
    pub fn unsplit(self, w: WriteHalf<T>) -> Result<T, UnsplitError<T>> {
        self.handle.reunite(w.handle).map_err(|e| {
            UnsplitError {
                read_half: ReadHalf { handle: e.0 },
                write_half: WriteHalf { handle: e.1 }
            }
        })
    }
}

/// The writable half of an object returned from `AsyncRead::split`.
#[derive(Debug)]
pub struct WriteHalf<T> {
    handle: BiLock<T>,
}

impl<T: AsyncRead + AsyncWrite> WriteHalf<T> {
    /// Reunite with a previously split `ReadHalf`.
    ///
    /// If this `WriteHalf` and the given `ReadHalf` do not originate from
    /// the same `AsyncRead::split` operation they will be returned, wrapped
    /// in an `UnsplitError`.
    pub fn unsplit(self, r: ReadHalf<T>) -> Result<T, UnsplitError<T>> {
        self.handle.reunite(r.handle).map_err(|e| {
            UnsplitError {
                read_half: ReadHalf { handle: e.0 },
                write_half: WriteHalf { handle: e.1 }
            }
        })
    }
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

/// Error returned from `unsplit` if reuniting a `ReadHalf` with
/// another `WriteHalf` fails.
pub struct UnsplitError<T> {
    /// The `ReadHalf` passed to `unsplit`.
    pub read_half: ReadHalf<T>,
    /// The `WriteHalf` passed to `unsplit`.
    pub write_half: WriteHalf<T>
}

impl<T> fmt::Debug for UnsplitError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("UnsplitError")
    }
}

impl<T> fmt::Display for UnsplitError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("tried to reunite unrelated read and write halves")
    }
}

impl<T> error::Error for UnsplitError<T> {
    fn description(&self) -> &str {
        "tried to reunite unrelated read and write halves"
    }
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

    #[test]
    fn unsplit_ok() {
        let (r, w) = RW.split();
        assert!(r.unsplit(w).is_ok());

        let (r, w) = RW.split();
        assert!(w.unsplit(r).is_ok())
    }

    #[test]
    fn unsplit_err() {
        let (r1, w1) = RW.split();
        let (r2, w2) = RW.split();
        assert!(r1.unsplit(w2).is_err());
        assert!(r2.unsplit(w1).is_err());

        let (r1, w1) = RW.split();
        let (r2, w2) = RW.split();
        assert!(w1.unsplit(r2).is_err());
        assert!(w2.unsplit(r1).is_err())
    }
}
