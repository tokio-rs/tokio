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
fn smoke() {
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

/*
#[test]
fn multiple_watches() {
    let (mut watch1, mut store) = Watch::new("one");
    let mut watch2 = watch1.clone();

    {
        let mut h1 = Harness::poll_fn(|| watch1.poll());
        let mut h2 = Harness::poll_fn(|| watch2.poll());

        assert!(!h1.poll().unwrap().is_ready());

        // Change the value.
        assert_eq!(store.store("two").unwrap(), "one");

        // The watch was notified
        assert!(h1.poll().unwrap().is_ready());
        assert!(h2.poll().unwrap().is_ready());
    }

    assert_eq!(*watch1.borrow(), "two");
    assert_eq!(*watch2.borrow(), "two");
}
*/
