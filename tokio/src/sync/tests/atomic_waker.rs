use crate::sync::AtomicWaker;
use tokio_test::task;

use std::task::Waker;

trait AssertSend: Send {}
trait AssertSync: Send {}

impl AssertSend for AtomicWaker {}
impl AssertSync for AtomicWaker {}

impl AssertSend for Waker {}
impl AssertSync for Waker {}

#[test]
fn basic_usage() {
    let mut waker = task::spawn(AtomicWaker::new());

    waker.enter(|cx, waker| waker.register_by_ref(cx.waker()));
    waker.wake();

    assert!(waker.is_woken());
}

#[test]
fn wake_without_register() {
    let mut waker = task::spawn(AtomicWaker::new());
    waker.wake();

    // Registering should not result in a notification
    waker.enter(|cx, waker| waker.register_by_ref(cx.waker()));

    assert!(!waker.is_woken());
}
