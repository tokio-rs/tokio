#![cfg(feature = "broken")]
#![deny(warnings, rust_2018_idioms)]

use futures::{future, Async, Future, Poll};
use tokio_macros::{assert_not_ready, assert_ready, assert_ready_eq};

#[test]
fn assert_ready() {
    let mut fut = future::ok::<(), ()>(());
    assert_ready!(fut.poll());
    let mut fut = future::ok::<(), ()>(());
    assert_ready!(fut.poll(), "some message");
}

#[test]
#[should_panic]
fn assert_ready_err() {
    let mut fut = future::err::<(), ()>(());
    assert_ready!(fut.poll());
}

#[test]
fn assert_not_ready() {
    let poll: Poll<(), ()> = Ok(Async::NotReady);
    assert_not_ready!(poll);
    assert_not_ready!(poll, "some message");
}

#[test]
#[should_panic]
fn assert_not_ready_err() {
    let mut fut = future::err::<(), ()>(());
    assert_not_ready!(fut.poll());
}

#[test]
fn assert_ready_eq() {
    let mut fut = future::ok::<(), ()>(());
    assert_ready_eq!(fut.poll(), ());
}
