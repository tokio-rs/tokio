extern crate tokio_io;
extern crate bytes;
extern crate futures;

use tokio_io::AsyncSeek;
use futures::Async;

use std::io::{self, Seek, SeekFrom};

#[test]
fn poll_seek_success() {
    struct S;

    impl Seek for S {
        fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
            Ok(11)
        }
    }

    impl AsyncSeek for S {}

    let n = match S.poll_seek(SeekFrom::Start(0)).unwrap() {
        Async::Ready(n) => n,
        _ => panic!(),
    };

    assert_eq!(11, n);
}

#[test]
fn poll_seek_error() {
    struct S;

    impl Seek for S {
        fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
            Err(io::Error::new(io::ErrorKind::Other, "other"))
        }
    }

    impl AsyncSeek for S {}

    let err = S.poll_seek(SeekFrom::Start(0)).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Other);
}

#[test]
fn poll_seek_translate_wouldblock_to_not_ready() {
    struct S;

    impl Seek for S {
        fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
            Err(io::Error::new(io::ErrorKind::WouldBlock, ""))
        }
    }

    impl AsyncSeek for S {}

    assert!(!S.poll_seek(SeekFrom::Start(0)).unwrap().is_ready());
}
