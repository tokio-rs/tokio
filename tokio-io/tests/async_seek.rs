extern crate tokio_io;
extern crate bytes;
extern crate futures;

use tokio_io::{io, AsyncSeek};
use futures::{Async, Poll, Future};

use std::io::{self as std_io, SeekFrom};

#[test]
fn poll_seek_success() {
    struct S;

    impl AsyncSeek for S {
        fn poll_seek(&mut self, _pos: SeekFrom) -> Poll<u64, std_io::Error> {
            Ok(Async::Ready(11))
        }
    }

    let n = match io::seek(S, SeekFrom::Start(0)).poll().unwrap() {
        Async::Ready(n) => n.1,
        _ => panic!(),
    };

    assert_eq!(11, n);
}

#[test]
fn poll_seek_error() {
    struct S;

    impl AsyncSeek for S {
        fn poll_seek(&mut self, _pos: SeekFrom) -> Poll<u64, std_io::Error> {
            Err(std_io::Error::new(std_io::ErrorKind::Other, "other"))
        }
    }

    let err = io::seek(S, SeekFrom::Start(0)).poll().err().unwrap();
    assert_eq!(err.kind(), std_io::ErrorKind::Other);
}

#[test]
fn poll_seek_not_ready() {
    struct S;

    impl AsyncSeek for S {
        fn poll_seek(&mut self, _pos: SeekFrom) -> Poll<u64, std_io::Error> {
            Ok(Async::NotReady)
        }
    }

    assert!(!io::seek(S, SeekFrom::Start(0)).poll().unwrap().is_ready());
}
