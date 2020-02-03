#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::sync::Notify;
use tokio_test::task::spawn;
use tokio_test::*;

trait AssertSend: Send + Sync {}
impl AssertSend for Notify {}

#[test]
fn notify_recv_one() {
    let notify = Notify::new();
    let mut recv = spawn(async { notify.recv().await });

    notify.notify_one();
    assert_ready!(recv.poll());
}

#[test]
fn recv_one_notify() {
    let notify = Notify::new();
    let mut recv = spawn(async { notify.recv().await });

    assert_pending!(recv.poll());

    notify.notify_one();
    assert!(recv.is_woken());
    assert_ready!(recv.poll());
}

#[test]
fn recv_multi_notify() {
    let notify = Notify::new();
    let mut recv1 = spawn(async { notify.recv().await });
    let mut recv2 = spawn(async { notify.recv().await });

    assert_pending!(recv1.poll());
    assert_pending!(recv2.poll());

    notify.notify_one();
    assert!(recv1.is_woken());
    assert!(!recv2.is_woken());

    assert_ready!(recv1.poll());
    assert_pending!(recv2.poll());
}

#[test]
fn notify_recv_multi() {
    let notify = Notify::new();

    notify.notify_one();

    let mut recv1 = spawn(async { notify.recv().await });
    let mut recv2 = spawn(async { notify.recv().await });

    assert_ready!(recv1.poll());
    assert_pending!(recv2.poll());

    notify.notify_one();

    assert!(recv2.is_woken());
    assert_ready!(recv2.poll());
}

#[test]
fn recv_drop_recv_notify() {
    let notify = Notify::new();
    let mut recv1 = spawn(async { notify.recv().await });
    let mut recv2 = spawn(async { notify.recv().await });

    assert_pending!(recv1.poll());

    drop(recv1);

    assert_pending!(recv2.poll());

    notify.notify_one();
    assert!(recv2.is_woken());
    assert_ready!(recv2.poll());
}

#[test]
fn recv_multi_notify_drop_one() {
    let notify = Notify::new();
    let mut recv1 = spawn(async { notify.recv().await });
    let mut recv2 = spawn(async { notify.recv().await });

    assert_pending!(recv1.poll());
    assert_pending!(recv2.poll());

    notify.notify_one();

    assert!(recv1.is_woken());
    assert!(!recv2.is_woken());

    drop(recv1);

    assert!(recv2.is_woken());
    assert_ready!(recv2.poll());
}
