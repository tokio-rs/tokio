#![cfg(feature = "util")]

extern crate bytes;
extern crate futures;
extern crate tokio_buf;

use bytes::{Buf, Bytes};
use futures::Async::*;
use futures::Future;
use tokio_buf::{BufStream, BufStreamExt};

#[macro_use]
mod support;

use support::*;

// ===== test `chain()` =====

#[test]
fn chain() {
    // Chain one with one
    //
    let mut bs = one("hello").chain(one("world"));

    assert_buf_eq!(bs.poll_buf(), "hello");
    assert_buf_eq!(bs.poll_buf(), "world");
    assert_none!(bs.poll_buf());

    // Chain multi with multi
    let mut bs = list(&["foo", "bar"]).chain(list(&["baz", "bok"]));

    assert_buf_eq!(bs.poll_buf(), "foo");
    assert_buf_eq!(bs.poll_buf(), "bar");
    assert_buf_eq!(bs.poll_buf(), "baz");
    assert_buf_eq!(bs.poll_buf(), "bok");
    assert_none!(bs.poll_buf());

    // Chain includes a not ready call
    //
    let mut bs = new_mock(&[Ok(Ready("foo")), Ok(NotReady), Ok(Ready("bar"))]).chain(one("baz"));

    assert_buf_eq!(bs.poll_buf(), "foo");
    assert_not_ready!(bs.poll_buf());
    assert_buf_eq!(bs.poll_buf(), "bar");
    assert_buf_eq!(bs.poll_buf(), "baz");
    assert_none!(bs.poll_buf());
}

// ===== Test `collect()` =====

macro_rules! test_collect_impl {
    ($t:ty $(, $capacity:ident)*) => {
        // While unfortunate, this test makes some assumptions on vec's resizing
        // behavior.
        //
        // Collect one
        //
        let bs = one("hello world");

        let vec: $t = bs.collect().wait().unwrap();

        assert_eq!(vec, &b"hello world"[..]);
        $( assert_eq!(vec.$capacity(), 64); )*

        // Collect one, with size hint
        //
        let mut bs = one("hello world");
        bs.size_hint.set_lower(11);

        let vec: $t = bs.collect().wait().unwrap();

        assert_eq!(vec, &b"hello world"[..]);
        $( assert_eq!(vec.$capacity(), 64); )*

        // Collect one, with size hint
        //
        let mut bs = one("hello world");
        bs.size_hint.set_lower(10);

        let vec: $t = bs.collect().wait().unwrap();

        assert_eq!(vec, &b"hello world"[..]);
        $( assert_eq!(vec.$capacity(), 64); )*

        // Collect many
        //
        let bs = list(&["hello", " ", "world", ", one two three"]);

        let vec: $t = bs.collect().wait().unwrap();

        assert_eq!(vec, &b"hello world, one two three"[..]);
    }
}

#[test]
fn collect_vec() {
    test_collect_impl!(Vec<u8>, capacity);
}

#[test]
fn collect_bytes() {
    test_collect_impl!(Bytes);
}

// ===== Test limit() =====

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
