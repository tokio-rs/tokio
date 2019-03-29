extern crate bytes;
extern crate futures;
extern crate tokio_buf;

use futures::Async::*;
use tokio_buf::{util, BufStream};
use std::io::Cursor;

#[macro_use]
mod support;

type Buf = Cursor<&'static [u8]>;

#[test]
fn empty_iter() {
    let mut bs = util::iter::<_, ()>(Vec::<Buf>::new());
    assert_none!(bs.poll_buf());
}

#[test]
fn full_iter() {
    let bufs = vec![
        buf(b"one"),
        buf(b"two"),
        buf(b"three"),
    ];

    let mut bs = util::iter::<_, ()>(bufs);
    assert_buf_eq!(bs.poll_buf(), "one");
    assert_buf_eq!(bs.poll_buf(), "two");
    assert_buf_eq!(bs.poll_buf(), "three");
    assert_none!(bs.poll_buf());
}

fn buf(data: &'static [u8]) -> Buf {
    Cursor::new(data)
}
