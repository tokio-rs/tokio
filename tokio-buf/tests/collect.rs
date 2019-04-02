#![cfg(feature = "util")]

extern crate bytes;
extern crate futures;
extern crate tokio_buf;

use bytes::Bytes;
use futures::Future;
use tokio_buf::BufStreamExt;

#[macro_use]
mod support;

use support::*;

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
