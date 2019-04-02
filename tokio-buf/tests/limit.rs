#![cfg(feature = "util")]

extern crate bytes;
extern crate futures;
extern crate tokio_buf;

use futures::Async::*;
use futures::Future;
use tokio_buf::{BufStream, BufStreamExt};

#[macro_use]
mod support;

use support::*;

#[test]
fn limit() {
    // Not limited

    let res = one("hello world")
        .limit(100)
        .collect::<Vec<_>>()
        .wait()
        .unwrap();

    assert_eq!(res, b"hello world");

    let res = list(&["hello", " ", "world"])
        .limit(100)
        .collect::<Vec<_>>()
        .wait()
        .unwrap();

    assert_eq!(res, b"hello world");

    let res = list(&["hello", " ", "world"])
        .limit(11)
        .collect::<Vec<_>>()
        .wait()
        .unwrap();

    assert_eq!(res, b"hello world");

    // Limited

    let res = one("hello world").limit(5).collect::<Vec<_>>().wait();

    assert!(res.is_err());

    let res = one("hello world").limit(10).collect::<Vec<_>>().wait();

    assert!(res.is_err());

    let mut bs = list(&["hello", " ", "world"]).limit(9);

    assert_buf_eq!(bs.poll_buf(), "hello");
    assert_buf_eq!(bs.poll_buf(), " ");
    assert!(bs.poll_buf().is_err());

    let mut bs = list(&["hello", " ", "world"]);
    bs.size_hint.set_lower(11);
    let mut bs = bs.limit(9);

    assert!(bs.poll_buf().is_err());
}
