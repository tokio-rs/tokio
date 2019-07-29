extern crate futures;
extern crate tokio_mock_task;
extern crate tokio_sync;

use tokio_mock_task::*;
use tokio_sync::lock::Lock;

macro_rules! assert_ready {
    ($e:expr) => {{
        match $e {
            futures::Async::Ready(v) => v,
            futures::Async::NotReady => panic!("not ready"),
        }
    }};
}

macro_rules! assert_not_ready {
    ($e:expr) => {{
        match $e {
            futures::Async::NotReady => {}
            futures::Async::Ready(v) => panic!("ready; value = {:?}", v),
        }
    }};
}

#[test]
fn straight_execution() {
    let mut l = Lock::new(100);

    // We can immediately acquire the lock and take the value
    let mut g = assert_ready!(l.poll_lock());
    assert_eq!(&*g, &100);
    *g = 99;
    drop(g);

    let mut g = assert_ready!(l.poll_lock());
    assert_eq!(&*g, &99);
    *g = 98;
    drop(g);

    let mut g = assert_ready!(l.poll_lock());
    assert_eq!(&*g, &98);

    // We can continue to access the guard even if the lock is dropped
    drop(l);
    *g = 97;
    assert_eq!(&*g, &97);
}

#[test]
fn readiness() {
    let mut task = MockTask::new();

    let mut l = Lock::new(100);
    let g = assert_ready!(l.poll_lock());

    // We can't now acquire the lease since it's already held in g
    task.enter(|| {
        assert_not_ready!(l.poll_lock());
    });

    // But once g unlocks, we can acquire it
    drop(g);
    assert!(task.is_notified());
    assert_ready!(l.poll_lock());
}
