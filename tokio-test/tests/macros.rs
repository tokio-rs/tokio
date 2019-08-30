#![warn(rust_2018_idioms)]

use std::task::Poll;
use tokio_test::{
    assert_pending, assert_ready, assert_ready_eq, assert_ready_err, assert_ready_ok,
};

fn ready() -> Poll<()> {
    Poll::Ready(())
}

fn ready_ok() -> Poll<Result<(), ()>> {
    Poll::Ready(Ok(()))
}

fn ready_err() -> Poll<Result<(), ()>> {
    Poll::Ready(Err(()))
}

fn pending() -> Poll<()> {
    Poll::Pending
}

#[derive(Debug)]
enum Test {
    Data,
}

#[test]
fn assert_ready() {
    let poll = ready();
    assert_ready!(poll);
    assert_ready!(poll, "some message");
    assert_ready!(poll, "{:?}", ());
    assert_ready!(poll, "{:?}", Test::Data);
}

#[test]
#[should_panic]
fn assert_ready_on_pending() {
    let poll = pending();
    assert_ready!(poll);
}

#[test]
fn assert_pending() {
    let poll = pending();
    assert_pending!(poll);
    assert_pending!(poll, "some message");
    assert_pending!(poll, "{:?}", ());
    assert_pending!(poll, "{:?}", Test::Data);
}

#[test]
#[should_panic]
fn assert_pending_on_ready() {
    let poll = ready();
    assert_pending!(poll);
}

#[test]
fn assert_ready_ok() {
    let poll = ready_ok();
    assert_ready_ok!(poll);
    assert_ready_ok!(poll, "some message");
    assert_ready_ok!(poll, "{:?}", ());
    assert_ready_ok!(poll, "{:?}", Test::Data);
}

#[test]
#[should_panic]
fn assert_ok_on_err() {
    let poll = ready_err();
    assert_ready_ok!(poll);
}

#[test]
fn assert_ready_err() {
    let poll = ready_err();
    assert_ready_err!(poll);
    assert_ready_err!(poll, "some message");
    assert_ready_err!(poll, "{:?}", ());
    assert_ready_err!(poll, "{:?}", Test::Data);
}

#[test]
#[should_panic]
fn assert_err_on_ok() {
    let poll = ready_ok();
    assert_ready_err!(poll);
}

#[test]
fn assert_ready_eq() {
    let poll = ready();
    assert_ready_eq!(poll, ());
    assert_ready_eq!(poll, (), "some message");
    assert_ready_eq!(poll, (), "{:?}", ());
    assert_ready_eq!(poll, (), "{:?}", Test::Data);
}

#[test]
#[should_panic]
fn assert_eq_on_not_eq() {
    let poll = ready_err();
    assert_ready_eq!(poll, Ok(()));
}
