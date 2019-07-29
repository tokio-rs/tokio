extern crate futures;
extern crate tokio_mock_task;
extern crate tokio_sync;

use tokio_mock_task::*;
use tokio_sync::mpsc;

use futures::prelude::*;

use std::sync::Arc;
use std::thread;

trait AssertSend: Send {}
impl AssertSend for mpsc::Sender<i32> {}
impl AssertSend for mpsc::Receiver<i32> {}

macro_rules! assert_ready {
    ($e:expr) => {{
        match $e {
            Ok(futures::Async::Ready(v)) => v,
            Ok(_) => panic!("not ready"),
            Err(e) => panic!("error = {:?}", e),
        }
    }};
}

macro_rules! assert_not_ready {
    ($e:expr) => {{
        match $e {
            Ok(futures::Async::NotReady) => {}
            Ok(futures::Async::Ready(v)) => panic!("ready; value = {:?}", v),
            Err(e) => panic!("error = {:?}", e),
        }
    }};
}

#[test]
fn send_recv_with_buffer() {
    let (mut tx, mut rx) = mpsc::channel::<i32>(16);

    // Using poll_ready / try_send
    assert_ready!(tx.poll_ready());
    tx.try_send(1).unwrap();

    // Without poll_ready
    tx.try_send(2).unwrap();

    // Sink API
    assert!(tx.start_send(3).unwrap().is_ready());
    assert_ready!(tx.poll_complete());
    assert_ready!(tx.close());

    drop(tx);

    let val = assert_ready!(rx.poll());
    assert_eq!(val, Some(1));

    let val = assert_ready!(rx.poll());
    assert_eq!(val, Some(2));

    let val = assert_ready!(rx.poll());
    assert_eq!(val, Some(3));

    let val = assert_ready!(rx.poll());
    assert!(val.is_none());
}

#[test]
fn start_send_past_cap() {
    let (mut tx1, mut rx) = mpsc::channel(1);
    let mut tx2 = tx1.clone();

    let mut task1 = MockTask::new();
    let mut task2 = MockTask::new();

    let res = tx1.start_send(()).unwrap();
    assert!(res.is_ready());

    task1.enter(|| {
        let res = tx1.start_send(()).unwrap();
        assert!(!res.is_ready());
    });

    task2.enter(|| {
        assert_not_ready!(tx2.poll_ready());
    });

    drop(tx1);

    let val = assert_ready!(rx.poll());
    assert!(val.is_some());

    assert!(task2.is_notified());
    assert!(!task1.is_notified());

    drop(tx2);

    let val = assert_ready!(rx.poll());
    assert!(val.is_none());
}

#[test]
#[should_panic]
fn buffer_gteq_one() {
    mpsc::channel::<i32>(0);
}

#[test]
fn send_recv_unbounded() {
    let (mut tx, mut rx) = mpsc::unbounded_channel::<i32>();

    // Using `try_send`
    tx.try_send(1).unwrap();

    // Using `Sink` API
    assert!(tx.start_send(2).unwrap().is_ready());
    assert_ready!(tx.poll_complete());

    let val = assert_ready!(rx.poll());
    assert_eq!(val, Some(1));

    let val = assert_ready!(rx.poll());
    assert_eq!(val, Some(2));

    assert_ready!(tx.poll_complete());
    assert_ready!(tx.close());

    drop(tx);

    let val = assert_ready!(rx.poll());
    assert!(val.is_none());
}

#[test]
fn no_t_bounds_buffer() {
    struct NoImpls;
    let (tx, mut rx) = mpsc::channel(100);

    // sender should be Debug even though T isn't Debug
    println!("{:?}", tx);
    // same with Receiver
    println!("{:?}", rx);
    // and sender should be Clone even though T isn't Clone
    assert!(tx.clone().try_send(NoImpls).is_ok());
    assert!(assert_ready!(rx.poll()).is_some());
}

#[test]
fn no_t_bounds_unbounded() {
    struct NoImpls;
    let (tx, mut rx) = mpsc::unbounded_channel();

    // sender should be Debug even though T isn't Debug
    println!("{:?}", tx);
    // same with Receiver
    println!("{:?}", rx);
    // and sender should be Clone even though T isn't Clone
    assert!(tx.clone().try_send(NoImpls).is_ok());
    assert!(assert_ready!(rx.poll()).is_some());
}

#[test]
fn send_recv_buffer_limited() {
    let (mut tx, mut rx) = mpsc::channel::<i32>(1);
    let mut task = MockTask::new();

    // Run on a task context
    task.enter(|| {
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
    });
}

#[test]
fn send_shared_recv() {
    let (tx1, rx) = mpsc::channel::<i32>(16);
    let tx2 = tx1.clone();
    let mut rx = rx.wait();

    tx1.send(1).wait().unwrap();
    assert_eq!(rx.next().unwrap().unwrap(), 1);

    tx2.send(2).wait().unwrap();
    assert_eq!(rx.next().unwrap().unwrap(), 2);
}

#[test]
fn send_recv_threads() {
    let (tx, rx) = mpsc::channel::<i32>(16);
    let mut rx = rx.wait();

    thread::spawn(move || {
        tx.send(1).wait().unwrap();
    });

    assert_eq!(rx.next().unwrap().unwrap(), 1);
}

#[test]
fn recv_close_gets_none_idle() {
    let (mut tx, mut rx) = mpsc::channel::<i32>(10);
    let mut task = MockTask::new();

    rx.close();

    task.enter(|| {
        let val = assert_ready!(rx.poll());
        assert!(val.is_none());
        assert!(tx.poll_ready().is_err());
    });
}

#[test]
fn recv_close_gets_none_reserved() {
    let (mut tx1, mut rx) = mpsc::channel::<i32>(1);
    let mut tx2 = tx1.clone();

    assert_ready!(tx1.poll_ready());

    let mut task = MockTask::new();

    task.enter(|| {
        assert_not_ready!(tx2.poll_ready());
    });

    rx.close();

    assert!(task.is_notified());

    task.enter(|| {
        assert!(tx2.poll_ready().is_err());
        assert_not_ready!(rx.poll());
    });

    assert!(!task.is_notified());

    assert!(tx1.try_send(123).is_ok());

    assert!(task.is_notified());

    task.enter(|| {
        let v = assert_ready!(rx.poll());
        assert_eq!(v, Some(123));

        let v = assert_ready!(rx.poll());
        assert!(v.is_none());
    });
}

#[test]
fn tx_close_gets_none() {
    let (_, mut rx) = mpsc::channel::<i32>(10);
    let mut task = MockTask::new();

    // Run on a task context
    task.enter(|| {
        let v = assert_ready!(rx.poll());
        assert!(v.is_none());
    });
}

fn is_ready<T>(res: &AsyncSink<T>) -> bool {
    match *res {
        AsyncSink::Ready => true,
        _ => false,
    }
}

#[test]
fn try_send_fail() {
    let (mut tx, rx) = mpsc::channel(1);
    let mut rx = rx.wait();

    tx.try_send("hello").unwrap();

    // This should fail
    assert!(tx.try_send("fail").unwrap_err().is_full());

    assert_eq!(rx.next().unwrap().unwrap(), "hello");

    tx.try_send("goodbye").unwrap();
    drop(tx);

    assert_eq!(rx.next().unwrap().unwrap(), "goodbye");
    assert!(rx.next().is_none());
}

#[test]
fn drop_tx_with_permit_releases_permit() {
    // poll_ready reserves capacity, ensure that the capacity is released if tx
    // is dropped w/o sending a value.
    let (mut tx1, _rx) = mpsc::channel::<i32>(1);
    let mut tx2 = tx1.clone();
    let mut task = MockTask::new();

    assert_ready!(tx1.poll_ready());

    task.enter(|| {
        assert_not_ready!(tx2.poll_ready());
    });

    drop(tx1);

    assert!(task.is_notified());

    assert_ready!(tx2.poll_ready());
}

#[test]
fn dropping_rx_closes_channel() {
    let (mut tx, rx) = mpsc::channel(100);

    let msg = Arc::new(());
    tx.try_send(msg.clone()).unwrap();

    drop(rx);
    assert!(tx.poll_ready().is_err());

    assert_eq!(1, Arc::strong_count(&msg));
}

#[test]
fn dropping_rx_closes_channel_for_try() {
    let (mut tx, rx) = mpsc::channel(100);

    let msg = Arc::new(());
    tx.try_send(msg.clone()).unwrap();

    drop(rx);
    assert!(tx.try_send(msg.clone()).unwrap_err().is_closed());

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
