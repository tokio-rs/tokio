#![warn(rust_2018_idioms)]
#![feature(async_await)]

use tokio_sync::oneshot;
use tokio_test::task::MockTask;
use tokio_test::*;

trait AssertSend: Send {}
impl AssertSend for oneshot::Sender<i32> {}
impl AssertSend for oneshot::Receiver<i32> {}

#[test]
fn send_recv() {
    let (tx, mut rx) = oneshot::channel();
    let mut task = MockTask::new();

    assert_pending!(task.poll(&mut rx));

    assert_ok!(tx.send(1));

    assert!(task.is_woken());

    let val = assert_ready_ok!(task.poll(&mut rx));
    assert_eq!(val, 1);
}

#[tokio::test]
async fn async_send_recv() {
    let (tx, rx) = oneshot::channel();

    assert_ok!(tx.send(1));
    assert_eq!(1, assert_ok!(rx.await));
}

#[test]
fn close_tx() {
    let (tx, mut rx) = oneshot::channel::<i32>();
    let mut task = MockTask::new();

    assert_pending!(task.poll(&mut rx));

    drop(tx);

    assert!(task.is_woken());
    assert_ready_err!(task.poll(&mut rx));
}

#[test]
fn close_rx() {
    // First, without checking poll_closed()
    //
    let (tx, _) = oneshot::channel();

    assert_err!(tx.send(1));

    // Second, via poll_closed();

    let (mut tx, rx) = oneshot::channel();
    let mut task = MockTask::new();

    assert_pending!(task.enter(|cx| tx.poll_closed(cx)));

    drop(rx);

    assert!(task.is_woken());
    assert!(tx.is_closed());
    assert_ready!(task.enter(|cx| tx.poll_closed(cx)));

    assert_err!(tx.send(1));
}

#[tokio::test]
async fn async_rx_closed() {
    let (mut tx, rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        drop(rx);
    });

    tx.closed().await;
}

#[test]
fn explicit_close_poll() {
    // First, with message sent
    let (tx, mut rx) = oneshot::channel();
    let mut task = MockTask::new();

    assert_ok!(tx.send(1));

    rx.close();

    let value = assert_ready_ok!(task.poll(&mut rx));
    assert_eq!(value, 1);

    // Second, without the message sent
    let (mut tx, mut rx) = oneshot::channel::<i32>();

    assert_pending!(task.enter(|cx| tx.poll_closed(cx)));

    rx.close();

    assert!(task.is_woken());
    assert!(tx.is_closed());
    assert_ready!(task.enter(|cx| tx.poll_closed(cx)));

    assert_err!(tx.send(1));
    assert_ready_err!(task.poll(&mut rx));

    // Again, but without sending the value this time
    let (mut tx, mut rx) = oneshot::channel::<i32>();
    let mut task = MockTask::new();

    assert_pending!(task.enter(|cx| tx.poll_closed(cx)));

    rx.close();

    assert!(task.is_woken());
    assert!(tx.is_closed());
    assert_ready!(task.enter(|cx| tx.poll_closed(cx)));

    assert_ready_err!(task.poll(&mut rx));
}

#[test]
fn explicit_close_try_recv() {
    // First, with message sent
    let (tx, mut rx) = oneshot::channel();

    assert_ok!(tx.send(1));

    rx.close();

    let val = assert_ok!(rx.try_recv());
    assert_eq!(1, val);

    // Second, without the message sent
    let (mut tx, mut rx) = oneshot::channel::<i32>();
    let mut task = MockTask::new();

    assert_pending!(task.enter(|cx| tx.poll_closed(cx)));

    rx.close();

    assert!(task.is_woken());
    assert!(tx.is_closed());
    assert_ready!(task.enter(|cx| tx.poll_closed(cx)));

    assert_err!(rx.try_recv());
}

#[test]
#[should_panic]
fn close_try_recv_poll() {
    let (_tx, mut rx) = oneshot::channel::<i32>();
    let mut task = MockTask::new();

    rx.close();

    assert_err!(rx.try_recv());

    let _ = task.poll(&mut rx);
}

#[test]
fn drops_tasks() {
    let (mut tx, mut rx) = oneshot::channel::<i32>();
    let mut tx_task = MockTask::new();
    let mut rx_task = MockTask::new();

    assert_pending!(tx_task.enter(|cx| tx.poll_closed(cx)));
    assert_pending!(rx_task.poll(&mut rx));

    drop(tx);
    drop(rx);

    assert_eq!(1, tx_task.waker_ref_count());
    assert_eq!(1, rx_task.waker_ref_count());
}

#[test]
fn receiver_changes_task() {
    let (tx, mut rx) = oneshot::channel();

    let mut task1 = MockTask::new();
    let mut task2 = MockTask::new();

    assert_pending!(task1.poll(&mut rx));

    assert_eq!(2, task1.waker_ref_count());
    assert_eq!(1, task2.waker_ref_count());

    assert_pending!(task2.poll(&mut rx));

    assert_eq!(1, task1.waker_ref_count());
    assert_eq!(2, task2.waker_ref_count());

    assert_ok!(tx.send(1));

    assert!(!task1.is_woken());
    assert!(task2.is_woken());

    assert_ready_ok!(task2.poll(&mut rx));
}

#[test]
fn sender_changes_task() {
    let (mut tx, rx) = oneshot::channel::<i32>();

    let mut task1 = MockTask::new();
    let mut task2 = MockTask::new();

    assert_pending!(task1.enter(|cx| tx.poll_closed(cx)));

    assert_eq!(2, task1.waker_ref_count());
    assert_eq!(1, task2.waker_ref_count());

    assert_pending!(task2.enter(|cx| tx.poll_closed(cx)));

    assert_eq!(1, task1.waker_ref_count());
    assert_eq!(2, task2.waker_ref_count());

    drop(rx);

    assert!(!task1.is_woken());
    assert!(task2.is_woken());

    assert_ready!(task2.enter(|cx| tx.poll_closed(cx)));
}
