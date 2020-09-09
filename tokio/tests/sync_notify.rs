#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::sync::Notify;
use tokio_test::task::spawn;
use tokio_test::*;

trait AssertSend: Send + Sync {}
impl AssertSend for Notify {}

#[test]
fn notify_notified_one() {
    let notify = Notify::new();
    let mut notified = spawn(async { notify.notified().await });

    notify.notify_one();
    assert_ready!(notified.poll());
}

#[test]
fn notified_one_notify() {
    let notify = Notify::new();
    let mut notified = spawn(async { notify.notified().await });

    assert_pending!(notified.poll());

    notify.notify_one();
    assert!(notified.is_woken());
    assert_ready!(notified.poll());
}

#[test]
fn notified_multi_notify() {
    let notify = Notify::new();
    let mut notified1 = spawn(async { notify.notified().await });
    let mut notified2 = spawn(async { notify.notified().await });

    assert_pending!(notified1.poll());
    assert_pending!(notified2.poll());

    notify.notify_one();
    assert!(notified1.is_woken());
    assert!(!notified2.is_woken());

    assert_ready!(notified1.poll());
    assert_pending!(notified2.poll());
}

#[test]
fn notify_notified_multi() {
    let notify = Notify::new();

    notify.notify_one();

    let mut notified1 = spawn(async { notify.notified().await });
    let mut notified2 = spawn(async { notify.notified().await });

    assert_ready!(notified1.poll());
    assert_pending!(notified2.poll());

    notify.notify_one();

    assert!(notified2.is_woken());
    assert_ready!(notified2.poll());
}

#[test]
fn notified_drop_notified_notify() {
    let notify = Notify::new();
    let mut notified1 = spawn(async { notify.notified().await });
    let mut notified2 = spawn(async { notify.notified().await });

    assert_pending!(notified1.poll());

    drop(notified1);

    assert_pending!(notified2.poll());

    notify.notify_one();
    assert!(notified2.is_woken());
    assert_ready!(notified2.poll());
}

#[test]
fn notified_multi_notify_drop_one() {
    let notify = Notify::new();
    let mut notified1 = spawn(async { notify.notified().await });
    let mut notified2 = spawn(async { notify.notified().await });

    assert_pending!(notified1.poll());
    assert_pending!(notified2.poll());

    notify.notify_one();

    assert!(notified1.is_woken());
    assert!(!notified2.is_woken());

    drop(notified1);

    assert!(notified2.is_woken());
    assert_ready!(notified2.poll());
}
