#![warn(rust_2018_idioms)]

use tokio::sync::AtomicWaker;
use tokio_test::task::MockTask;

use std::task::Waker;

trait AssertSend: Send {}
trait AssertSync: Send {}

impl AssertSend for AtomicWaker {}
impl AssertSync for AtomicWaker {}

impl AssertSend for Waker {}
impl AssertSync for Waker {}

#[test]
fn basic_usage() {
    let waker = AtomicWaker::new();
    let mut task = MockTask::new();

    task.enter(|cx| waker.register_by_ref(cx.waker()));
    waker.wake();

    assert!(task.is_woken());
}

#[test]
fn wake_without_register() {
    let waker = AtomicWaker::new();
    waker.wake();

    // Registering should not result in a notification
    let mut task = MockTask::new();
    task.enter(|cx| waker.register_by_ref(cx.waker()));

    assert!(!task.is_woken());
}
