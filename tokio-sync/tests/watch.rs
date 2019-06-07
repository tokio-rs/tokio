#![deny(warnings, rust_2018_idioms)]

use tokio_sync::watch;
use tokio_test::task::MockTask;
use tokio_test::{assert_pending, assert_ready};

/*
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
*/

#[test]
fn single_rx_poll_ref() {
    let (tx, mut rx) = watch::channel("one");
    let mut task = MockTask::new();

    task.enter(|cx| {
        {
            let v = assert_ready!(rx.poll_ref(cx)).unwrap();
            assert_eq!(*v, "one");
        }
        assert_pending!(rx.poll_ref(cx));
    });

    tx.broadcast("two").unwrap();

    assert!(task.is_woken());

    task.enter(|cx| {
        {
            let v = assert_ready!(rx.poll_ref(cx)).unwrap();
            assert_eq!(*v, "two");
        }
        assert_pending!(rx.poll_ref(cx));
    });

    drop(tx);

    assert!(task.is_woken());

    task.enter(|cx| {
        let res = assert_ready!(rx.poll_ref(cx));
        assert!(res.is_none());
    });
}

#[test]
fn single_rx_poll_next() {
    let (tx, mut rx) = watch::channel("one");
    let mut task = MockTask::new();

    task.enter(|cx| {
        let v = assert_ready!(rx.poll_next(cx)).unwrap();
        assert_eq!(v, "one");
        assert_pending!(rx.poll_ref(cx));
    });

    tx.broadcast("two").unwrap();

    assert!(task.is_woken());

    task.enter(|cx| {
        let v = assert_ready!(rx.poll_next(cx)).unwrap();
        assert_eq!(v, "two");
        assert_pending!(rx.poll_ref(cx));
    });

    drop(tx);

    assert!(task.is_woken());

    task.enter(|cx| {
        let res = assert_ready!(rx.poll_next(cx));
        assert!(res.is_none());
    });
}

#[test]
#[cfg(feature = "async-traits")]
fn stream_impl() {
    use futures_core::Stream;
    use pin_utils::pin_mut;

    let (tx, rx) = watch::channel("one");
    let mut task = MockTask::new();

    pin_mut!(rx);

    task.enter(|cx| {
        {
            let v = assert_ready!(Stream::poll_next(rx.as_mut(), cx)).unwrap();
            assert_eq!(v, "one");
        }
        assert_pending!(rx.poll_ref(cx));
    });

    tx.broadcast("two").unwrap();

    assert!(task.is_woken());

    task.enter(|cx| {
        {
            let v = assert_ready!(Stream::poll_next(rx.as_mut(), cx)).unwrap();
            assert_eq!(v, "two");
        }
        assert_pending!(rx.poll_ref(cx));
    });

    drop(tx);

    assert!(task.is_woken());

    task.enter(|cx| {
        let res = assert_ready!(Stream::poll_next(rx, cx));
        assert!(res.is_none());
    });
}

#[test]
fn multi_rx() {
    let (tx, mut rx1) = watch::channel("one");
    let mut rx2 = rx1.clone();

    let mut task1 = MockTask::new();
    let mut task2 = MockTask::new();

    task1.enter(|cx| {
        let res = assert_ready!(rx1.poll_ref(cx));
        assert_eq!(*res.unwrap(), "one");
    });

    task2.enter(|cx| {
        let res = assert_ready!(rx2.poll_ref(cx));
        assert_eq!(*res.unwrap(), "one");
    });

    tx.broadcast("two").unwrap();

    assert!(task1.is_woken());
    assert!(task2.is_woken());

    task1.enter(|cx| {
        let res = assert_ready!(rx1.poll_ref(cx));
        assert_eq!(*res.unwrap(), "two");
    });

    tx.broadcast("three").unwrap();

    assert!(task1.is_woken());
    assert!(task2.is_woken());

    task1.enter(|cx| {
        let res = assert_ready!(rx1.poll_ref(cx));
        assert_eq!(*res.unwrap(), "three");
    });

    task2.enter(|cx| {
        let res = assert_ready!(rx2.poll_ref(cx));
        assert_eq!(*res.unwrap(), "three");
    });

    tx.broadcast("four").unwrap();

    task1.enter(|cx| {
        let res = assert_ready!(rx1.poll_ref(cx));
        assert_eq!(*res.unwrap(), "four");
    });

    drop(tx);

    task1.enter(|cx| {
        let res = assert_ready!(rx1.poll_ref(cx));
        assert!(res.is_none());
    });

    task2.enter(|cx| {
        let res = assert_ready!(rx2.poll_ref(cx));
        assert_eq!(*res.unwrap(), "four");
    });

    task2.enter(|cx| {
        let res = assert_ready!(rx2.poll_ref(cx));
        assert!(res.is_none());
    });
}

#[test]
fn rx_observes_final_value() {
    // Initial value

    let (tx, mut rx) = watch::channel("one");
    let mut task = MockTask::new();

    drop(tx);

    task.enter(|cx| {
        let res = assert_ready!(rx.poll_ref(cx));
        assert!(res.is_some());
        assert_eq!(*res.unwrap(), "one");
    });

    task.enter(|cx| {
        let res = assert_ready!(rx.poll_ref(cx));
        assert!(res.is_none());
    });

    // Sending a value

    let (tx, mut rx) = watch::channel("one");
    let mut task = MockTask::new();

    tx.broadcast("two").unwrap();

    task.enter(|cx| {
        {
            let res = assert_ready!(rx.poll_ref(cx));
            assert!(res.is_some());
            assert_eq!(*res.unwrap(), "two");
        }

        assert_pending!(rx.poll_ref(cx));
    });

    tx.broadcast("three").unwrap();
    drop(tx);

    assert!(task.is_woken());

    task.enter(|cx| {
        let res = assert_ready!(rx.poll_ref(cx));
        assert!(res.is_some());
        assert_eq!(*res.unwrap(), "three");
    });

    task.enter(|cx| {
        let res = assert_ready!(rx.poll_ref(cx));
        assert!(res.is_none());
    });
}

#[test]
fn poll_close() {
    let (mut tx, rx) = watch::channel("one");
    let mut task = MockTask::new();

    assert_pending!(task.enter(|cx| tx.poll_close(cx)));

    drop(rx);

    assert!(task.is_woken());

    assert_ready!(task.enter(|cx| tx.poll_close(cx)));

    assert!(tx.broadcast("two").is_err());
}
