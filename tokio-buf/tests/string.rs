extern crate bytes;
extern crate futures;
extern crate tokio_buf;

use futures::Async::*;
use std::fmt;
use tokio_buf::BufStream;

#[macro_use]
mod support;

fn test_hello_world<B>(mut bs: B)
where
    B: BufStream + fmt::Debug,
    B::Item: fmt::Debug,
    B::Error: fmt::Debug,
{
    let hint = bs.size_hint();
    assert_eq!(hint.lower(), 11);
    assert_eq!(hint.upper(), Some(11));

    assert_buf_eq!(bs.poll_buf(), "hello world");

    let hint = bs.size_hint();
    assert_eq!(hint.lower(), 0);
    assert_eq!(hint.upper(), Some(0));
    assert_none!(bs.poll_buf());
}

#[test]
fn string() {
    test_hello_world("hello world".to_string());
}

#[test]
fn str() {
    test_hello_world("hello world");
}
