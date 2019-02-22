extern crate tokio_mock_task;
extern crate tokio_sync;

use tokio_mock_task::*;
use tokio_sync::watch;

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
fn single_rx() {
    let (mut tx, mut rx) = watch::channel("one");
    let mut task = MockTask::new();

    task.enter(|| {
        let v = assert_ready!(rx.poll()).unwrap();
        assert_eq!(*v, "one");
    });

    task.enter(|| assert_not_ready!(rx.poll()));

    assert!(!task.is_notified());

    tx.send("two").unwrap();

    assert!(task.is_notified());

    task.enter(|| {
        let v = assert_ready!(rx.poll()).unwrap();
        assert_eq!(*v, "two");
    });

    task.enter(|| assert_not_ready!(rx.poll()));

    drop(tx);

    assert!(task.is_notified());

    task.enter(|| {
        let res = assert_ready!(rx.poll());
        assert!(res.is_none());
    });
}

#[test]
fn multi_rx() {
    let (mut tx, mut rx1) = watch::channel("one");
    let mut rx2 = rx1.clone();

    let mut task1 = MockTask::new();
    let mut task2 = MockTask::new();

    task1.enter(|| {
        let res = assert_ready!(rx1.poll());
        assert_eq!(*res.unwrap(), "one");
    });

    task2.enter(|| {
        let res = assert_ready!(rx2.poll());
        assert_eq!(*res.unwrap(), "one");
    });

    tx.send("two").unwrap();

    assert!(task1.is_notified());
    assert!(task2.is_notified());

    task1.enter(|| {
        let res = assert_ready!(rx1.poll());
        assert_eq!(*res.unwrap(), "two");
    });

    tx.send("three").unwrap();

    assert!(task1.is_notified());
    assert!(task2.is_notified());

    task1.enter(|| {
        let res = assert_ready!(rx1.poll());
        assert_eq!(*res.unwrap(), "three");
    });

    task2.enter(|| {
        let res = assert_ready!(rx2.poll());
        assert_eq!(*res.unwrap(), "three");
    });

    tx.send("four").unwrap();

    task1.enter(|| {
        let res = assert_ready!(rx1.poll());
        assert_eq!(*res.unwrap(), "four");
    });

    drop(tx);

    task1.enter(|| {
        let res = assert_ready!(rx1.poll());
        assert!(res.is_none());
    });

    task2.enter(|| {
        let res = assert_ready!(rx2.poll());
        assert_eq!(*res.unwrap(), "four");
    });

    task2.enter(|| {
        let res = assert_ready!(rx2.poll());
        assert!(res.is_none());
    });
}

#[test]
fn rx_observes_final_value() {
    // Initial value

    let (tx, mut rx) = watch::channel("one");
    let mut task = MockTask::new();

    drop(tx);

    task.enter(|| {
        let res = assert_ready!(rx.poll());
        assert!(res.is_some());
        assert_eq!(*res.unwrap(), "one");
    });

    task.enter(|| {
        let res = assert_ready!(rx.poll());
        assert!(res.is_none());
    });

    // Sending a value

    let (mut tx, mut rx) = watch::channel("one");
    let mut task = MockTask::new();

    tx.send("two").unwrap();

    task.enter(|| {
        let res = assert_ready!(rx.poll());
        assert!(res.is_some());
        assert_eq!(*res.unwrap(), "two");
    });

    task.enter(|| assert_not_ready!(rx.poll()));

    tx.send("three").unwrap();
    drop(tx);

    assert!(task.is_notified());

    task.enter(|| {
        let res = assert_ready!(rx.poll());
        assert!(res.is_some());
        assert_eq!(*res.unwrap(), "three");
    });

    task.enter(|| {
        let res = assert_ready!(rx.poll());
        assert!(res.is_none());
    });
}
