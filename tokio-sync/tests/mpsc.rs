extern crate tokio_sync;
#[macro_use]
extern crate futures;

use tokio_sync::mpsc;

use futures::prelude::*;
use futures::future::lazy;

use std::thread;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

trait AssertSend: Send {}
impl AssertSend for mpsc::Sender<i32> {}
impl AssertSend for mpsc::Receiver<i32> {}

#[test]
fn send_recv() {
    let (tx, rx) = mpsc::channel::<i32>(16);
    let mut rx = rx.wait();

    tx.send(1).wait().unwrap();

    assert_eq!(rx.next().unwrap(), Ok(1));
}

#[test]
fn send_recv_no_buffer() {
    let (mut tx, mut rx) = mpsc::channel::<i32>(0);

    // Run on a task context
    lazy(move || {
        assert!(tx.poll_complete().unwrap().is_ready());
        assert!(tx.poll_ready().unwrap().is_ready());

        // Send first message
        let res = tx.start_send(1).unwrap();
        assert!(is_ready(&res));
        assert!(tx.poll_ready().unwrap().is_not_ready());

        // Send second message
        let res = tx.start_send(2).unwrap();
        assert!(!is_ready(&res));

        // Take the value
        assert_eq!(rx.poll().unwrap(), Async::Ready(Some(1)));
        assert!(tx.poll_ready().unwrap().is_ready());

        let res = tx.start_send(2).unwrap();
        assert!(is_ready(&res));
        assert!(tx.poll_ready().unwrap().is_not_ready());

        // Take the value
        assert_eq!(rx.poll().unwrap(), Async::Ready(Some(2)));
        assert!(tx.poll_ready().unwrap().is_ready());

        Ok::<(), ()>(())
    }).wait().unwrap();
}

#[test]
fn send_shared_recv() {
    let (tx1, rx) = mpsc::channel::<i32>(16);
    let tx2 = tx1.clone();
    let mut rx = rx.wait();

    tx1.send(1).wait().unwrap();
    assert_eq!(rx.next().unwrap(), Ok(1));

    tx2.send(2).wait().unwrap();
    assert_eq!(rx.next().unwrap(), Ok(2));
}

#[test]
fn send_recv_threads() {
    let (tx, rx) = mpsc::channel::<i32>(16);
    let mut rx = rx.wait();

    thread::spawn(move|| {
        tx.send(1).wait().unwrap();
    });

    assert_eq!(rx.next().unwrap(), Ok(1));
}

#[test]
#[ignore]
fn send_recv_threads_no_capacity() {
    let (tx, rx) = mpsc::channel::<i32>(0);
    let mut rx = rx.wait();

    let (readytx, readyrx) = mpsc::channel::<()>(2);
    let mut readyrx = readyrx.wait();
    let t = thread::spawn(move|| {
        let readytx = readytx.sink_map_err(|_| panic!());
        let (a, b) = tx.send(1).join(readytx.send(())).wait().unwrap();
        a.send(2).join(b.send(())).wait().unwrap();
    });

    drop(readyrx.next().unwrap());
    assert_eq!(rx.next().unwrap(), Ok(1));
    drop(readyrx.next().unwrap());
    assert_eq!(rx.next().unwrap(), Ok(2));

    t.join().unwrap();
}

#[test]
fn recv_close_gets_none() {
    let (mut tx, mut rx) = mpsc::channel::<i32>(10);

    // Run on a task context
    lazy(move || {
        rx.close();

        assert_eq!(rx.poll(), Ok(Async::Ready(None)));
        assert!(tx.poll_ready().is_err());

        drop(tx);

        Ok::<(), ()>(())
    }).wait().unwrap();
}


#[test]
fn tx_close_gets_none() {
    let (_, mut rx) = mpsc::channel::<i32>(10);

    // Run on a task context
    lazy(move || {
        assert_eq!(rx.poll(), Ok(Async::Ready(None)));
        assert_eq!(rx.poll(), Ok(Async::Ready(None)));

        Ok::<(), ()>(())
    }).wait().unwrap();
}

fn is_ready<T>(res: &AsyncSink<T>) -> bool {
    match *res {
        AsyncSink::Ready => true,
        _ => false,
    }
}

#[test]
#[ignore]
fn try_send_1() {
    const N: usize = 3000;
    let (mut tx, rx) = mpsc::channel(0);

    let t = thread::spawn(move || {
        for i in 0..N {
            loop {
                if tx.try_send(i).is_ok() {
                    break
                }
            }
        }
    });
    for (i, j) in rx.wait().enumerate() {
        assert_eq!(i, j.unwrap());
    }
    t.join().unwrap();
}

/*
#[test]
fn try_send_2() {
    let (mut tx, rx) = mpsc::channel(0);

    tx.try_send("hello").unwrap();

    let (readytx, readyrx) = oneshot::channel::<()>();

    let th = thread::spawn(|| {
        lazy(|| {
            assert!(tx.start_send("fail").unwrap().is_not_ready());
            Ok::<_, ()>(())
        }).wait().unwrap();

        drop(readytx);
        tx.send("goodbye").wait().unwrap();
    });

    let mut rx = rx.wait();

    drop(readyrx.wait());
    assert_eq!(rx.next(), Some(Ok("hello")));
    assert_eq!(rx.next(), Some(Ok("goodbye")));
    assert!(rx.next().is_none());

    th.join().unwrap();
}
*/

#[test]
fn try_send_fail() {
    let (mut tx, rx) = mpsc::channel(0);
    let mut rx = rx.wait();

    tx.try_send("hello").unwrap();

    // This should fail
    assert!(tx.try_send("fail").is_err());

    assert_eq!(rx.next(), Some(Ok("hello")));

    tx.try_send("goodbye").unwrap();
    drop(tx);

    assert_eq!(rx.next(), Some(Ok("goodbye")));
    assert!(rx.next().is_none());
}

#[test]
#[ignore]
fn drop_tx_with_permit_releases_permit() {
    // poll_ready reserves capacity, ensure that the capacity is released if tx
    // is dropped w/o sending a value.
}
