#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::sync::oneshot;
use tokio_test::*;

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

trait AssertSend: Send {}
impl AssertSend for oneshot::Sender<i32> {}
impl AssertSend for oneshot::Receiver<i32> {}

trait SenderExt {
    fn poll_closed(&mut self, cx: &mut Context<'_>) -> Poll<()>;
}
impl<T> SenderExt for oneshot::Sender<T> {
    fn poll_closed(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        tokio::pin! {
            let fut = self.closed();
        }
        fut.poll(cx)
    }
}

#[test]
fn send_recv() {
    let (tx, rx) = oneshot::channel();
    let mut rx = task::spawn(rx);

    assert_pending!(rx.poll());

    assert_ok!(tx.send(1));

    assert!(rx.is_woken());

    let val = assert_ready_ok!(rx.poll());
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
    let (tx, rx) = oneshot::channel::<i32>();
    let mut rx = task::spawn(rx);

    assert_pending!(rx.poll());

    drop(tx);

    assert!(rx.is_woken());
    assert_ready_err!(rx.poll());
}

#[test]
fn close_rx() {
    // First, without checking poll_closed()
    //
    let (tx, _) = oneshot::channel();

    assert_err!(tx.send(1));

    // Second, via poll_closed();

    let (tx, rx) = oneshot::channel();
    let mut tx = task::spawn(tx);

    assert_pending!(tx.enter(|cx, mut tx| tx.poll_closed(cx)));

    drop(rx);

    assert!(tx.is_woken());
    assert!(tx.is_closed());
    assert_ready!(tx.enter(|cx, mut tx| tx.poll_closed(cx)));

    assert_err!(tx.into_inner().send(1));
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
    let (tx, rx) = oneshot::channel();
    let mut rx = task::spawn(rx);

    assert_ok!(tx.send(1));

    rx.close();

    let value = assert_ready_ok!(rx.poll());
    assert_eq!(value, 1);

    // Second, without the message sent
    let (tx, rx) = oneshot::channel::<i32>();
    let mut tx = task::spawn(tx);
    let mut rx = task::spawn(rx);

    assert_pending!(tx.enter(|cx, mut tx| tx.poll_closed(cx)));

    rx.close();

    assert!(tx.is_woken());
    assert!(tx.is_closed());
    assert_ready!(tx.enter(|cx, mut tx| tx.poll_closed(cx)));

    assert_err!(tx.into_inner().send(1));
    assert_ready_err!(rx.poll());

    // Again, but without sending the value this time
    let (tx, rx) = oneshot::channel::<i32>();
    let mut tx = task::spawn(tx);
    let mut rx = task::spawn(rx);

    assert_pending!(tx.enter(|cx, mut tx| tx.poll_closed(cx)));

    rx.close();

    assert!(tx.is_woken());
    assert!(tx.is_closed());
    assert_ready!(tx.enter(|cx, mut tx| tx.poll_closed(cx)));

    assert_ready_err!(rx.poll());
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
    let (tx, mut rx) = oneshot::channel::<i32>();
    let mut tx = task::spawn(tx);

    assert_pending!(tx.enter(|cx, mut tx| tx.poll_closed(cx)));

    rx.close();

    assert!(tx.is_woken());
    assert!(tx.is_closed());
    assert_ready!(tx.enter(|cx, mut tx| tx.poll_closed(cx)));

    assert_err!(rx.try_recv());
}

#[test]
#[should_panic]
fn close_try_recv_poll() {
    let (_tx, rx) = oneshot::channel::<i32>();
    let mut rx = task::spawn(rx);

    rx.close();

    assert_err!(rx.try_recv());

    let _ = rx.poll();
}

#[test]
fn drops_tasks() {
    let (mut tx, mut rx) = oneshot::channel::<i32>();
    let mut tx_task = task::spawn(());
    let mut rx_task = task::spawn(());

    assert_pending!(tx_task.enter(|cx, _| tx.poll_closed(cx)));
    assert_pending!(rx_task.enter(|cx, _| Pin::new(&mut rx).poll(cx)));

    drop(tx);
    drop(rx);

    assert_eq!(1, tx_task.waker_ref_count());
    assert_eq!(1, rx_task.waker_ref_count());
}

#[test]
fn receiver_changes_task() {
    let (tx, mut rx) = oneshot::channel();

    let mut task1 = task::spawn(());
    let mut task2 = task::spawn(());

    assert_pending!(task1.enter(|cx, _| Pin::new(&mut rx).poll(cx)));

    assert_eq!(2, task1.waker_ref_count());
    assert_eq!(1, task2.waker_ref_count());

    assert_pending!(task2.enter(|cx, _| Pin::new(&mut rx).poll(cx)));

    assert_eq!(1, task1.waker_ref_count());
    assert_eq!(2, task2.waker_ref_count());

    assert_ok!(tx.send(1));

    assert!(!task1.is_woken());
    assert!(task2.is_woken());

    assert_ready_ok!(task2.enter(|cx, _| Pin::new(&mut rx).poll(cx)));
}

#[test]
fn sender_changes_task() {
    let (mut tx, rx) = oneshot::channel::<i32>();

    let mut task1 = task::spawn(());
    let mut task2 = task::spawn(());

    assert_pending!(task1.enter(|cx, _| tx.poll_closed(cx)));

    assert_eq!(2, task1.waker_ref_count());
    assert_eq!(1, task2.waker_ref_count());

    assert_pending!(task2.enter(|cx, _| tx.poll_closed(cx)));

    assert_eq!(1, task1.waker_ref_count());
    assert_eq!(2, task2.waker_ref_count());

    drop(rx);

    assert!(!task1.is_woken());
    assert!(task2.is_woken());

    assert_ready!(task2.enter(|cx, _| tx.poll_closed(cx)));
}
