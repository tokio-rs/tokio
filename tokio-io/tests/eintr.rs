extern crate tokio_io;
extern crate bytes;
extern crate futures;

use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::io::shutdown;

use bytes::{BytesMut};
use futures::{Async, Poll, Future};

use std::io::{self, Read, Write, Cursor, Error, ErrorKind};

#[test]
//#[ignore]
fn eintr_async_read() {
    struct R {
        pub call_n: usize // increment each call to `read`
    }

    impl Read for R {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.call_n += 1;
            if self.call_n == 1 {
                // this is the first call, return emulated EINTR
                Err(Error::from(ErrorKind::Interrupted))
            } else {
                buf[0..11].copy_from_slice(b"hello world");
                Ok(11)
            }
        }
    }

    impl AsyncRead for R {}

    let mut reader = R { call_n: 0 };

    let mut buf = BytesMut::with_capacity(65);
    let n = reader.read_buf(&mut buf).unwrap();

    assert_eq!(Async::Ready(11), n);
    assert_eq!(buf[..], b"hello world"[..]);
}


#[test]
fn eintr_async_write() {
    struct W {
        pub buf: BytesMut,
        pub call_n: usize // increment each call to `write`
    }

    impl Write for W {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.call_n += 1;
            if self.call_n == 1 {
                // this is the first call, return emulated EINTR
                Err(Error::from(ErrorKind::Interrupted))
            } else {
                self.buf.extend_from_slice(buf);
                Ok(buf.len())
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl AsyncWrite for W {
        fn shutdown(&mut self) -> Poll<(), io::Error> {
            Ok(().into())
        }
    }

    let mut writer = W { buf: BytesMut::new(), call_n: 0 };

    let mut buf = bytes::Buf::take(Cursor::new("hello world"), 11);
    let n = writer.write_buf(&mut buf).unwrap();

    assert_eq!(Async::Ready(11), n);
    assert_eq!(writer.buf[..], b"hello world"[..]);
}

#[test]
fn eintr_shutdown() {
    #[derive(Debug, PartialEq)]
    struct W {
        pub call_n: usize // increment each call to `shutdown`
    }

    impl Write for W {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Ok(0)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl AsyncWrite for W {
        fn shutdown(&mut self) -> Poll<(), io::Error> {
            self.call_n += 1;
            if self.call_n == 1 {
                // this is the first call, return emulated EINTR
                Err(Error::from(ErrorKind::Interrupted))
            } else {
                Ok(().into())
            }
        }
    }

    let writer = W { call_n: 0 };
    assert_eq!(W { call_n: 1}, shutdown(writer).wait().unwrap());
}
