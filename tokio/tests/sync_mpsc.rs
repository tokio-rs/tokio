#![allow(clippy::redundant_clone)]
#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::sync::mpsc;
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio_test::task;
use tokio_test::{
    assert_err, assert_ok, assert_pending, assert_ready, assert_ready_err, assert_ready_ok,
};

use std::sync::Arc;

trait AssertSend: Send {}
impl AssertSend for mpsc::Sender<i32> {}
impl AssertSend for mpsc::Receiver<i32> {}

#[test]
fn send_recv_with_buffer() {
    let (tx, rx) = mpsc::channel::<i32>(16);
    let mut tx = task::spawn(tx);
    let mut rx = task::spawn(rx);

    // Using poll_ready / try_send
    assert_ready_ok!(tx.enter(|cx, mut tx| tx.poll_ready(cx)));
    tx.try_send(1).unwrap();

    // Without poll_ready
    tx.try_send(2).unwrap();

    drop(tx);

    let val = assert_ready!(rx.enter(|cx, mut rx| rx.poll_recv(cx)));
    assert_eq!(val, Some(1));

    let val = assert_ready!(rx.enter(|cx, mut rx| rx.poll_recv(cx)));
    assert_eq!(val, Some(2));

    let val = assert_ready!(rx.enter(|cx, mut rx| rx.poll_recv(cx)));
    assert!(val.is_none());
}

#[test]
fn disarm() {
    let (tx, rx) = mpsc::channel::<i32>(2);
    let mut tx1 = task::spawn(tx.clone());
    let mut tx2 = task::spawn(tx.clone());
    let mut tx3 = task::spawn(tx.clone());
    let mut tx4 = task::spawn(tx);
    let mut rx = task::spawn(rx);

    // We should be able to `poll_ready` two handles without problem
    assert_ready_ok!(tx1.enter(|cx, mut tx| tx.poll_ready(cx)));
    assert_ready_ok!(tx2.enter(|cx, mut tx| tx.poll_ready(cx)));

    // But a third should not be ready
    assert_pending!(tx3.enter(|cx, mut tx| tx.poll_ready(cx)));

    // Using one of the reserved slots should allow a new handle to become ready
    tx1.try_send(1).unwrap();
    // We also need to receive for the slot to be free
    let _ = assert_ready!(rx.enter(|cx, mut rx| rx.poll_recv(cx))).unwrap();
    // Now there's a free slot!
    assert_ready_ok!(tx3.enter(|cx, mut tx| tx.poll_ready(cx)));
    assert_pending!(tx4.enter(|cx, mut tx| tx.poll_ready(cx)));

    // Dropping a ready handle should also open up a slot
    drop(tx2);
    assert_ready_ok!(tx4.enter(|cx, mut tx| tx.poll_ready(cx)));
    assert_pending!(tx1.enter(|cx, mut tx| tx.poll_ready(cx)));

    // Explicitly disarming a handle should also open a slot
    assert!(tx3.disarm());
    assert_ready_ok!(tx1.enter(|cx, mut tx| tx.poll_ready(cx)));

    // Disarming a non-armed sender does not free up a slot
    assert!(!tx3.disarm());
    assert_pending!(tx3.enter(|cx, mut tx| tx.poll_ready(cx)));
}

#[tokio::test]
async fn send_recv_stream_with_buffer() {
    use tokio::stream::StreamExt;

    let (mut tx, mut rx) = mpsc::channel::<i32>(16);

    tokio::spawn(async move {
        assert_ok!(tx.send(1).await);
        assert_ok!(tx.send(2).await);
    });

    assert_eq!(Some(1), rx.next().await);
    assert_eq!(Some(2), rx.next().await);
    assert_eq!(None, rx.next().await);
}

#[tokio::test]
async fn async_send_recv_with_buffer() {
    let (mut tx, mut rx) = mpsc::channel(16);

    tokio::spawn(async move {
        assert_ok!(tx.send(1).await);
        assert_ok!(tx.send(2).await);
    });

    assert_eq!(Some(1), rx.recv().await);
    assert_eq!(Some(2), rx.recv().await);
    assert_eq!(None, rx.recv().await);
}

#[test]
fn start_send_past_cap() {
    let mut t1 = task::spawn(());
    let mut t2 = task::spawn(());
    let mut t3 = task::spawn(());

    let (mut tx1, mut rx) = mpsc::channel(1);
    let mut tx2 = tx1.clone();

    assert_ok!(tx1.try_send(()));

    t1.enter(|cx, _| {
        assert_pending!(tx1.poll_ready(cx));
    });

    t2.enter(|cx, _| {
        assert_pending!(tx2.poll_ready(cx));
    });

    drop(tx1);

    let val = t3.enter(|cx, _| assert_ready!(rx.poll_recv(cx)));
    assert!(val.is_some());

    assert!(t2.is_woken());
    assert!(!t1.is_woken());

    drop(tx2);

    let val = t3.enter(|cx, _| assert_ready!(rx.poll_recv(cx)));
    assert!(val.is_none());
}

#[test]
#[should_panic]
fn buffer_gteq_one() {
    mpsc::channel::<i32>(0);
}

#[test]
fn send_recv_unbounded() {
    let mut t1 = task::spawn(());

    let (tx, mut rx) = mpsc::unbounded_channel::<i32>();

    // Using `try_send`
    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));

    let val = assert_ready!(t1.enter(|cx, _| rx.poll_recv(cx)));
    assert_eq!(val, Some(1));

    let val = assert_ready!(t1.enter(|cx, _| rx.poll_recv(cx)));
    assert_eq!(val, Some(2));

    drop(tx);

    let val = assert_ready!(t1.enter(|cx, _| rx.poll_recv(cx)));
    assert!(val.is_none());
}

#[tokio::test]
async fn async_send_recv_unbounded() {
    let (tx, mut rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        assert_ok!(tx.send(1));
        assert_ok!(tx.send(2));
    });

    assert_eq!(Some(1), rx.recv().await);
    assert_eq!(Some(2), rx.recv().await);
    assert_eq!(None, rx.recv().await);
}

#[tokio::test]
async fn send_recv_stream_unbounded() {
    use tokio::stream::StreamExt;

    let (tx, mut rx) = mpsc::unbounded_channel::<i32>();

    tokio::spawn(async move {
        assert_ok!(tx.send(1));
        assert_ok!(tx.send(2));
    });

    assert_eq!(Some(1), rx.next().await);
    assert_eq!(Some(2), rx.next().await);
    assert_eq!(None, rx.next().await);
}

#[test]
fn no_t_bounds_buffer() {
    struct NoImpls;

    let mut t1 = task::spawn(());
    let (tx, mut rx) = mpsc::channel(100);

    // sender should be Debug even though T isn't Debug
    println!("{:?}", tx);
    // same with Receiver
    println!("{:?}", rx);
    // and sender should be Clone even though T isn't Clone
    assert!(tx.clone().try_send(NoImpls).is_ok());

    let val = assert_ready!(t1.enter(|cx, _| rx.poll_recv(cx)));
    assert!(val.is_some());
}

#[test]
fn no_t_bounds_unbounded() {
    struct NoImpls;

    let mut t1 = task::spawn(());
    let (tx, mut rx) = mpsc::unbounded_channel();

    // sender should be Debug even though T isn't Debug
    println!("{:?}", tx);
    // same with Receiver
    println!("{:?}", rx);
    // and sender should be Clone even though T isn't Clone
    assert!(tx.clone().send(NoImpls).is_ok());

    let val = assert_ready!(t1.enter(|cx, _| rx.poll_recv(cx)));
    assert!(val.is_some());
}

#[test]
fn send_recv_buffer_limited() {
    let mut t1 = task::spawn(());
    let mut t2 = task::spawn(());

    let (mut tx, mut rx) = mpsc::channel::<i32>(1);

    // Run on a task context
    t1.enter(|cx, _| {
        assert_ready_ok!(tx.poll_ready(cx));

        // Send first message
        assert_ok!(tx.try_send(1));

        // Not ready
        assert_pending!(tx.poll_ready(cx));

        // Send second message
        assert_err!(tx.try_send(1337));
    });

    t2.enter(|cx, _| {
        // Take the value
        let val = assert_ready!(rx.poll_recv(cx));
        assert_eq!(Some(1), val);
    });

    assert!(t1.is_woken());

    t1.enter(|cx, _| {
        assert_ready_ok!(tx.poll_ready(cx));

        assert_ok!(tx.try_send(2));

        // Not ready
        assert_pending!(tx.poll_ready(cx));
    });

    t2.enter(|cx, _| {
        // Take the value
        let val = assert_ready!(rx.poll_recv(cx));
        assert_eq!(Some(2), val);
    });

    t1.enter(|cx, _| {
        assert_ready_ok!(tx.poll_ready(cx));
    });
}

#[test]
fn recv_close_gets_none_idle() {
    let mut t1 = task::spawn(());

    let (mut tx, mut rx) = mpsc::channel::<i32>(10);

    rx.close();

    t1.enter(|cx, _| {
        let val = assert_ready!(rx.poll_recv(cx));
        assert!(val.is_none());
        assert_ready_err!(tx.poll_ready(cx));
    });
}

#[test]
fn recv_close_gets_none_reserved() {
    let mut t1 = task::spawn(());
    let mut t2 = task::spawn(());
    let mut t3 = task::spawn(());

    let (mut tx1, mut rx) = mpsc::channel::<i32>(1);
    let mut tx2 = tx1.clone();

    assert_ready_ok!(t1.enter(|cx, _| tx1.poll_ready(cx)));

    t2.enter(|cx, _| {
        assert_pending!(tx2.poll_ready(cx));
    });

    rx.close();

    assert!(t2.is_woken());

    t2.enter(|cx, _| {
        assert_ready_err!(tx2.poll_ready(cx));
    });

    t3.enter(|cx, _| assert_pending!(rx.poll_recv(cx)));

    assert!(!t1.is_woken());
    assert!(!t2.is_woken());

    assert_ok!(tx1.try_send(123));

    assert!(t3.is_woken());

    t3.enter(|cx, _| {
        let v = assert_ready!(rx.poll_recv(cx));
        assert_eq!(v, Some(123));

        let v = assert_ready!(rx.poll_recv(cx));
        assert!(v.is_none());
    });
}

#[test]
fn tx_close_gets_none() {
    let mut t1 = task::spawn(());

    let (_, mut rx) = mpsc::channel::<i32>(10);

    // Run on a task context
    t1.enter(|cx, _| {
        let v = assert_ready!(rx.poll_recv(cx));
        assert!(v.is_none());
    });
}

#[test]
fn try_send_fail() {
    let mut t1 = task::spawn(());

    let (mut tx, mut rx) = mpsc::channel(1);

    tx.try_send("hello").unwrap();

    // This should fail
    match assert_err!(tx.try_send("fail")) {
        TrySendError::Full(..) => {}
        _ => panic!(),
    }

    let val = assert_ready!(t1.enter(|cx, _| rx.poll_recv(cx)));
    assert_eq!(val, Some("hello"));

    assert_ok!(tx.try_send("goodbye"));
    drop(tx);

    let val = assert_ready!(t1.enter(|cx, _| rx.poll_recv(cx)));
    assert_eq!(val, Some("goodbye"));

    let val = assert_ready!(t1.enter(|cx, _| rx.poll_recv(cx)));
    assert!(val.is_none());
}

#[test]
fn drop_tx_with_permit_releases_permit() {
    let mut t1 = task::spawn(());
    let mut t2 = task::spawn(());

    // poll_ready reserves capacity, ensure that the capacity is released if tx
    // is dropped w/o sending a value.
    let (mut tx1, _rx) = mpsc::channel::<i32>(1);
    let mut tx2 = tx1.clone();

    assert_ready_ok!(t1.enter(|cx, _| tx1.poll_ready(cx)));

    t2.enter(|cx, _| {
        assert_pending!(tx2.poll_ready(cx));
    });

    drop(tx1);

    assert!(t2.is_woken());

    assert_ready_ok!(t2.enter(|cx, _| tx2.poll_ready(cx)));
}

#[test]
fn dropping_rx_closes_channel() {
    let mut t1 = task::spawn(());

    let (mut tx, rx) = mpsc::channel(100);

    let msg = Arc::new(());
    assert_ok!(tx.try_send(msg.clone()));

    drop(rx);
    assert_ready_err!(t1.enter(|cx, _| tx.poll_ready(cx)));

    assert_eq!(1, Arc::strong_count(&msg));
}

#[test]
fn dropping_rx_closes_channel_for_try() {
    let (mut tx, rx) = mpsc::channel(100);

    let msg = Arc::new(());
    tx.try_send(msg.clone()).unwrap();

    drop(rx);

    {
        let err = assert_err!(tx.try_send(msg.clone()));
        match err {
            TrySendError::Closed(..) => {}
            _ => panic!(),
        }
    }

    assert_eq!(1, Arc::strong_count(&msg));
}

#[test]
fn unconsumed_messages_are_dropped() {
    let msg = Arc::new(());

    let (mut tx, rx) = mpsc::channel(100);

    tx.try_send(msg.clone()).unwrap();

    assert_eq!(2, Arc::strong_count(&msg));

    drop((tx, rx));

    assert_eq!(1, Arc::strong_count(&msg));
}

#[test]
fn try_recv() {
    let (mut tx, mut rx) = mpsc::channel(1);
    match rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        _ => panic!(),
    }
    tx.try_send(42).unwrap();
    match rx.try_recv() {
        Ok(42) => {}
        _ => panic!(),
    }
    drop(tx);
    match rx.try_recv() {
        Err(TryRecvError::Closed) => {}
        _ => panic!(),
    }
}

#[test]
fn try_recv_unbounded() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    match rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        _ => panic!(),
    }
    tx.send(42).unwrap();
    match rx.try_recv() {
        Ok(42) => {}
        _ => panic!(),
    }
    drop(tx);
    match rx.try_recv() {
        Err(TryRecvError::Closed) => {}
        _ => panic!(),
    }
}
