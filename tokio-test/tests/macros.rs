#![warn(rust_2018_idioms)]

use std::task::Poll;
use tokio_test::{assert_pending, assert_ready, assert_ready_eq};

fn ready() -> Poll<()> {
    Poll::Ready(())
}

fn pending() -> Poll<()> {
    Poll::Pending
}

#[test]
fn assert_ready() {
    let fut = ready();
    assert_ready!(fut);
    let fut = ready();
    assert_ready!(fut, "some message");
}

#[test]
#[should_panic]
fn assert_ready_err() {
    let fut = pending();
    assert_ready!(fut);
}

#[test]
fn assert_pending() {
    let poll = pending();
    assert_pending!(poll);
    assert_pending!(poll, "some message");
}

#[test]
fn assert_ready_eq() {
    let fut = ready();
    assert_ready_eq!(fut, ());
}
