extern crate tokio_buf;
extern crate bytes;
extern crate futures;

use tokio_buf::{BufStream, SizeHint};
use bytes::Buf;
use futures::Async::*;


#[macro_use]
mod support;

// ===== test `SizeHint` =====

#[test]
fn size_hint() {
    let hint = SizeHint::new();
    assert_eq!(hint.lower(), 0);
    assert!(hint.upper().is_none());

    let mut hint = SizeHint::new();
    hint.set_lower(100);
    assert_eq!(hint.lower(), 100);
    assert!(hint.upper().is_none());

    let mut hint = SizeHint::new();
    hint.set_upper(200);
    assert_eq!(hint.lower(), 0);
    assert_eq!(hint.upper(), Some(200));

    let mut hint = SizeHint::new();
    hint.set_lower(100);
    hint.set_upper(100);
    assert_eq!(hint.lower(), 100);
    assert_eq!(hint.upper(), Some(100));
}

#[test]
#[should_panic]
fn size_hint_lower_bigger_than_upper() {
    let mut hint = SizeHint::new();
    hint.set_upper(100);
    hint.set_lower(200);
}

#[test]
#[should_panic]
fn size_hint_upper_less_than_lower() {
    let mut hint = SizeHint::new();
    hint.set_lower(200);
    hint.set_upper(100);
}

// ===== BufStream impelmentations for misc types =====

#[test]
fn str_buf_stream() {
    let mut bs = "hello world".to_string();
    assert_buf_eq!(bs.poll_buf(), "hello world");
    assert!(bs.is_empty());
    assert_none!(bs.poll_buf());

    let mut bs = "hello world";
    assert_buf_eq!(bs.poll_buf(), "hello world");
    assert!(bs.is_empty());
    assert_none!(bs.poll_buf());
}
