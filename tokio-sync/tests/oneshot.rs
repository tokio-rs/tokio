extern crate futures;
extern crate tokio_mock_task;
extern crate tokio_sync;

use tokio_sync::oneshot;
use tokio_mock_task::*;

use futures::prelude::*;

macro_rules! assert_ready {
    ($e:expr) => {{
        match $e {
            Ok(futures::Async::Ready(v)) => v,
            Ok(_) => panic!("not ready"),
            Err(e) => panic!("error = {:?}", e),
        }
    }}
}

macro_rules! assert_not_ready {
    ($e:expr) => {{
        match $e {
            Ok(futures::Async::NotReady) => {},
            Ok(futures::Async::Ready(v)) => panic!("ready; value = {:?}", v),
            Err(e) => panic!("error = {:?}", e),
        }
    }}
}

#[test]
fn send_recv() {
    let (tx, mut rx) = oneshot::channel();
    let mut task = MockTask::new();

    task.enter(|| {
        assert_not_ready!(rx.poll());
    });

    tx.send(1).unwrap();

    assert!(task.is_notified());

    let val = assert_ready!(rx.poll());
    assert_eq!(val, 1);
}

#[test]
fn cancel_tx() {
    let (tx, mut rx) = oneshot::channel::<i32>();
    let mut task = MockTask::new();

    task.enter(|| {
        assert_not_ready!(rx.poll());
    });

    drop(tx);

    assert!(task.is_notified());

    rx.poll().unwrap_err();
}

#[test]
fn cancel_rx() {
    // First, without checking poll_cancel()
    //
    let (tx, _) = oneshot::channel();

    assert!(tx.send(1).is_err());

    // Second, via poll_cancel();

    let (mut tx, rx) = oneshot::channel::<i32>();
    let mut task = MockTask::new();

    task.enter(|| assert_not_ready!(tx.poll_cancel()));

    drop(rx);

    assert!(task.is_notified());
    assert!(tx.is_canceled());
    assert_ready!(tx.poll_cancel());
}
