extern crate futures;
extern crate tokio_mock_task;
extern crate tokio_sync;

use tokio_mock_task::*;
use tokio_sync::oneshot;

use futures::prelude::*;

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

trait AssertSend: Send {}
impl AssertSend for oneshot::Sender<i32> {}
impl AssertSend for oneshot::Receiver<i32> {}

#[test]
fn send_recv() {
    let (tx, mut rx) = oneshot::channel();
    let mut task = MockTask::new();

    task.enter(|| {
        assert_not_ready!(rx.poll());
    });

    assert!(tx.send(1).is_ok());

    assert!(task.is_notified());

    let val = assert_ready!(rx.poll());
    assert_eq!(val, 1);
}

#[test]
fn close_tx() {
    let (tx, mut rx) = oneshot::channel::<i32>();
    let mut task = MockTask::new();

    task.enter(|| {
        assert_not_ready!(rx.poll());
    });

    drop(tx);

    assert!(task.is_notified());
    assert!(rx.poll().is_err());
}

#[test]
fn close_rx() {
    // First, without checking poll_close()
    //
    let (tx, _) = oneshot::channel();

    assert!(tx.send(1).is_err());

    // Second, via poll_close();

    let (mut tx, rx) = oneshot::channel();
    let mut task = MockTask::new();

    task.enter(|| assert_not_ready!(tx.poll_close()));

    drop(rx);

    assert!(task.is_notified());
    assert!(tx.is_closed());
    assert_ready!(tx.poll_close());

    assert!(tx.send(1).is_err());
}

#[test]
fn explicit_close_poll() {
    // First, with message sent
    let (tx, mut rx) = oneshot::channel();

    assert!(tx.send(1).is_ok());

    rx.close();

    let value = assert_ready!(rx.poll());
    assert_eq!(value, 1);

    println!("~~~~~~~~~ TWO ~~~~~~~~~~");

    // Second, without the message sent
    let (mut tx, mut rx) = oneshot::channel::<i32>();
    let mut task = MockTask::new();

    task.enter(|| assert_not_ready!(tx.poll_close()));

    rx.close();

    assert!(task.is_notified());
    assert!(tx.is_closed());
    assert_ready!(tx.poll_close());

    assert!(tx.send(1).is_err());

    assert!(rx.poll().is_err());

    // Again, but without sending the value this time
    let (mut tx, mut rx) = oneshot::channel::<i32>();
    let mut task = MockTask::new();

    task.enter(|| assert_not_ready!(tx.poll_close()));

    rx.close();

    assert!(task.is_notified());
    assert!(tx.is_closed());
    assert_ready!(tx.poll_close());

    assert!(rx.poll().is_err());
}

#[test]
fn explicit_close_try_recv() {
    // First, with message sent
    let (tx, mut rx) = oneshot::channel();

    assert!(tx.send(1).is_ok());

    rx.close();

    assert_eq!(rx.try_recv().unwrap(), 1);

    println!("~~~~~~~~~ TWO ~~~~~~~~~~");

    // Second, without the message sent
    let (mut tx, mut rx) = oneshot::channel::<i32>();
    let mut task = MockTask::new();

    task.enter(|| assert_not_ready!(tx.poll_close()));

    rx.close();

    assert!(task.is_notified());
    assert!(tx.is_closed());
    assert_ready!(tx.poll_close());

    assert!(rx.try_recv().is_err());
}

#[test]
#[should_panic]
fn close_try_recv_poll() {
    let (_tx, mut rx) = oneshot::channel::<i32>();
    let mut task = MockTask::new();

    rx.close();

    assert!(rx.try_recv().is_err());

    task.enter(|| {
        let _ = rx.poll();
    });
}

#[test]
fn drops_tasks() {
    let (mut tx, mut rx) = oneshot::channel::<i32>();
    let mut tx_task = MockTask::new();
    let mut rx_task = MockTask::new();

    tx_task.enter(|| {
        assert_not_ready!(tx.poll_close());
    });

    rx_task.enter(|| {
        assert_not_ready!(rx.poll());
    });

    drop(tx);
    drop(rx);

    assert_eq!(1, tx_task.notifier_ref_count());
    assert_eq!(1, rx_task.notifier_ref_count());
}

#[test]
fn receiver_changes_task() {
    let (tx, mut rx) = oneshot::channel();

    let mut task1 = MockTask::new();
    let mut task2 = MockTask::new();

    task1.enter(|| {
        assert_not_ready!(rx.poll());
    });

    assert_eq!(2, task1.notifier_ref_count());
    assert_eq!(1, task2.notifier_ref_count());

    task2.enter(|| {
        assert_not_ready!(rx.poll());
    });

    assert_eq!(1, task1.notifier_ref_count());
    assert_eq!(2, task2.notifier_ref_count());

    tx.send(1).unwrap();

    assert!(!task1.is_notified());
    assert!(task2.is_notified());

    assert_ready!(rx.poll());
}

#[test]
fn sender_changes_task() {
    let (mut tx, rx) = oneshot::channel::<i32>();

    let mut task1 = MockTask::new();
    let mut task2 = MockTask::new();

    task1.enter(|| {
        assert_not_ready!(tx.poll_close());
    });

    assert_eq!(2, task1.notifier_ref_count());
    assert_eq!(1, task2.notifier_ref_count());

    task2.enter(|| {
        assert_not_ready!(tx.poll_close());
    });

    assert_eq!(1, task1.notifier_ref_count());
    assert_eq!(2, task2.notifier_ref_count());

    drop(rx);

    assert!(!task1.is_notified());
    assert!(task2.is_notified());

    assert_ready!(tx.poll_close());
}
