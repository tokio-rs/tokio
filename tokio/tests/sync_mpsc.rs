#![allow(clippy::redundant_clone)]
#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::thread;
use tokio::runtime::Runtime;
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

#[tokio::test]
async fn send_recv_with_buffer() {
    let (tx, mut rx) = mpsc::channel::<i32>(16);

    // Using poll_ready / try_send
    // let permit assert_ready_ok!(tx.reserve());
    let permit = tx.reserve().await.unwrap();
    permit.send(1);

    // Without poll_ready
    tx.try_send(2).unwrap();

    drop(tx);

    let val = rx.recv().await;
    assert_eq!(val, Some(1));

    let val = rx.recv().await;
    assert_eq!(val, Some(2));

    let val = rx.recv().await;
    assert!(val.is_none());
}

#[tokio::test]
async fn reserve_disarm() {
    let (tx, mut rx) = mpsc::channel::<i32>(2);
    let tx1 = tx.clone();
    let tx2 = tx.clone();
    let tx3 = tx.clone();
    let tx4 = tx;

    // We should be able to `poll_ready` two handles without problem
    let permit1 = assert_ok!(tx1.reserve().await);
    let permit2 = assert_ok!(tx2.reserve().await);

    // But a third should not be ready
    let mut r3 = task::spawn(tx3.reserve());
    assert_pending!(r3.poll());

    let mut r4 = task::spawn(tx4.reserve());
    assert_pending!(r4.poll());

    // Using one of the reserved slots should allow a new handle to become ready
    permit1.send(1);

    // We also need to receive for the slot to be free
    assert!(!r3.is_woken());
    rx.recv().await.unwrap();
    // Now there's a free slot!
    assert!(r3.is_woken());
    assert!(!r4.is_woken());

    // Dropping a permit should also open up a slot
    drop(permit2);
    assert!(r4.is_woken());

    let mut r1 = task::spawn(tx1.reserve());
    assert_pending!(r1.poll());
}

#[tokio::test]
async fn send_recv_stream_with_buffer() {
    use tokio::stream::StreamExt;

    let (tx, mut rx) = mpsc::channel::<i32>(16);

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
    let (tx, mut rx) = mpsc::channel(16);

    tokio::spawn(async move {
        assert_ok!(tx.send(1).await);
        assert_ok!(tx.send(2).await);
    });

    assert_eq!(Some(1), rx.recv().await);
    assert_eq!(Some(2), rx.recv().await);
    assert_eq!(None, rx.recv().await);
}

#[tokio::test]
async fn start_send_past_cap() {
    use std::future::Future;

    let mut t1 = task::spawn(());

    let (tx1, mut rx) = mpsc::channel(1);
    let tx2 = tx1.clone();

    assert_ok!(tx1.try_send(()));

    let mut r1 = Box::pin(tx1.reserve());
    t1.enter(|cx, _| assert_pending!(r1.as_mut().poll(cx)));

    {
        let mut r2 = task::spawn(tx2.reserve());
        assert_pending!(r2.poll());

        drop(r1);

        assert!(rx.recv().await.is_some());

        assert!(r2.is_woken());
        assert!(!t1.is_woken());
    }

    drop(tx1);
    drop(tx2);

    assert!(rx.recv().await.is_none());
}

#[test]
#[should_panic]
fn buffer_gteq_one() {
    mpsc::channel::<i32>(0);
}

#[tokio::test]
async fn send_recv_unbounded() {
    let (tx, mut rx) = mpsc::unbounded_channel::<i32>();

    // Using `try_send`
    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));

    assert_eq!(rx.recv().await, Some(1));
    assert_eq!(rx.recv().await, Some(2));

    drop(tx);

    assert!(rx.recv().await.is_none());
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

#[tokio::test]
async fn no_t_bounds_buffer() {
    struct NoImpls;

    let (tx, mut rx) = mpsc::channel(100);

    // sender should be Debug even though T isn't Debug
    println!("{:?}", tx);
    // same with Receiver
    println!("{:?}", rx);
    // and sender should be Clone even though T isn't Clone
    assert!(tx.clone().try_send(NoImpls).is_ok());

    assert!(rx.recv().await.is_some());
}

#[tokio::test]
async fn no_t_bounds_unbounded() {
    struct NoImpls;

    let (tx, mut rx) = mpsc::unbounded_channel();

    // sender should be Debug even though T isn't Debug
    println!("{:?}", tx);
    // same with Receiver
    println!("{:?}", rx);
    // and sender should be Clone even though T isn't Clone
    assert!(tx.clone().send(NoImpls).is_ok());

    assert!(rx.recv().await.is_some());
}

#[tokio::test]
async fn send_recv_buffer_limited() {
    let (tx, mut rx) = mpsc::channel::<i32>(1);

    // Reserve capacity
    let p1 = assert_ok!(tx.reserve().await);

    // Send first message
    p1.send(1);

    // Not ready
    let mut p2 = task::spawn(tx.reserve());
    assert_pending!(p2.poll());

    // Take the value
    assert!(rx.recv().await.is_some());

    // Notified
    assert!(p2.is_woken());

    // Trying to send fails
    assert_err!(tx.try_send(1337));

    // Send second
    let permit = assert_ready_ok!(p2.poll());
    permit.send(2);

    assert!(rx.recv().await.is_some());
}

#[tokio::test]
async fn recv_close_gets_none_idle() {
    let (tx, mut rx) = mpsc::channel::<i32>(10);

    rx.close();

    assert!(rx.recv().await.is_none());

    assert_err!(tx.send(1).await);
}

#[tokio::test]
async fn recv_close_gets_none_reserved() {
    let (tx1, mut rx) = mpsc::channel::<i32>(1);
    let tx2 = tx1.clone();

    let permit1 = assert_ok!(tx1.reserve().await);
    let mut permit2 = task::spawn(tx2.reserve());
    assert_pending!(permit2.poll());

    rx.close();

    assert!(permit2.is_woken());
    assert_ready_err!(permit2.poll());

    {
        let mut recv = task::spawn(rx.recv());
        assert_pending!(recv.poll());

        permit1.send(123);
        assert!(recv.is_woken());

        let v = assert_ready!(recv.poll());
        assert_eq!(v, Some(123));
    }

    assert!(rx.recv().await.is_none());
}

#[tokio::test]
async fn tx_close_gets_none() {
    let (_, mut rx) = mpsc::channel::<i32>(10);
    assert!(rx.recv().await.is_none());
}

#[tokio::test]
async fn try_send_fail() {
    let (tx, mut rx) = mpsc::channel(1);

    tx.try_send("hello").unwrap();

    // This should fail
    match assert_err!(tx.try_send("fail")) {
        TrySendError::Full(..) => {}
        _ => panic!(),
    }

    assert_eq!(rx.recv().await, Some("hello"));

    assert_ok!(tx.try_send("goodbye"));
    drop(tx);

    assert_eq!(rx.recv().await, Some("goodbye"));
    assert!(rx.recv().await.is_none());
}

#[tokio::test]
async fn drop_permit_releases_permit() {
    // poll_ready reserves capacity, ensure that the capacity is released if tx
    // is dropped w/o sending a value.
    let (tx1, _rx) = mpsc::channel::<i32>(1);
    let tx2 = tx1.clone();

    let permit = assert_ok!(tx1.reserve().await);

    let mut reserve2 = task::spawn(tx2.reserve());
    assert_pending!(reserve2.poll());

    drop(permit);

    assert!(reserve2.is_woken());
    assert_ready_ok!(reserve2.poll());
}

#[tokio::test]
async fn dropping_rx_closes_channel() {
    let (tx, rx) = mpsc::channel(100);

    let msg = Arc::new(());
    assert_ok!(tx.try_send(msg.clone()));

    drop(rx);
    assert_err!(tx.reserve().await);
    assert_eq!(1, Arc::strong_count(&msg));
}

#[test]
fn dropping_rx_closes_channel_for_try() {
    let (tx, rx) = mpsc::channel(100);

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

    let (tx, rx) = mpsc::channel(100);

    tx.try_send(msg.clone()).unwrap();

    assert_eq!(2, Arc::strong_count(&msg));

    drop((tx, rx));

    assert_eq!(1, Arc::strong_count(&msg));
}

#[test]
fn try_recv() {
    let (tx, mut rx) = mpsc::channel(1);
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

#[test]
fn blocking_recv() {
    let (tx, mut rx) = mpsc::channel::<u8>(1);

    let sync_code = thread::spawn(move || {
        assert_eq!(Some(10), rx.blocking_recv());
    });

    Runtime::new().unwrap().block_on(async move {
        let _ = tx.send(10).await;
    });
    sync_code.join().unwrap()
}

#[tokio::test]
#[should_panic]
async fn blocking_recv_async() {
    let (_tx, mut rx) = mpsc::channel::<()>(1);
    let _ = rx.blocking_recv();
}

#[test]
fn blocking_send() {
    let (tx, mut rx) = mpsc::channel::<u8>(1);

    let sync_code = thread::spawn(move || {
        tx.blocking_send(10).unwrap();
    });

    Runtime::new().unwrap().block_on(async move {
        assert_eq!(Some(10), rx.recv().await);
    });
    sync_code.join().unwrap()
}

#[tokio::test]
#[should_panic]
async fn blocking_send_async() {
    let (tx, _rx) = mpsc::channel::<()>(1);
    let _ = tx.blocking_send(());
}

#[tokio::test]
async fn ready_close_cancel_bounded() {
    let (tx, mut rx) = mpsc::channel::<()>(100);
    let _tx2 = tx.clone();

    let permit = assert_ok!(tx.reserve().await);

    rx.close();

    let mut recv = task::spawn(rx.recv());
    assert_pending!(recv.poll());

    drop(permit);

    assert!(recv.is_woken());
    let val = assert_ready!(recv.poll());
    assert!(val.is_none());
}

#[tokio::test]
async fn permit_available_not_acquired_close() {
    let (tx1, mut rx) = mpsc::channel::<()>(1);
    let tx2 = tx1.clone();

    let permit1 = assert_ok!(tx1.reserve().await);

    let mut permit2 = task::spawn(tx2.reserve());
    assert_pending!(permit2.poll());

    rx.close();

    drop(permit1);
    assert!(permit2.is_woken());

    drop(permit2);
    assert!(rx.recv().await.is_none());
}
