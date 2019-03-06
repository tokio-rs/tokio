#![deny(warnings)]

extern crate futures;
extern crate tokio_mock_task;
extern crate tokio_sync;

use tokio_mock_task::*;
use tokio_sync::lease::Lease;

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
    let mut l = Lease::from(100);

    // We can immediately acquire the lease and take the value
    assert_ready!(l.poll_acquire());
    assert_eq!(&*l, &100);
    assert_eq!(l.take(), 100);
    l.restore(99);

    // We can immediately acquire again since the value was returned
    assert_ready!(l.poll_acquire());
    assert_eq!(l.take(), 99);
    l.restore(98);

    // Dropping the lease is okay since we returned the value
    drop(l);
}

#[test]
fn drop_while_acquired_ok() {
    let mut l = Lease::from(100);
    assert_ready!(l.poll_acquire());

    // Dropping the lease while it is still acquired shouldn't
    // be an issue since we haven't taken the leased value.
    drop(l);
}

#[test]
#[should_panic]
fn take_twice() {
    let mut l = Lease::from(100);

    assert_ready!(l.poll_acquire());
    assert_eq!(l.take(), 100);
    l.take(); // should panic
}

#[test]
#[should_panic]
fn take_wo_acquire() {
    let mut l = Lease::from(100);
    l.take(); // should panic
}

#[test]
#[should_panic]
fn drop_without_restore() {
    let mut l = Lease::from(100);
    assert_ready!(l.poll_acquire());
    assert_eq!(l.take(), 100);
    drop(l); // should panic
}

#[test]
#[should_panic]
fn release_after_take() {
    let mut l = Lease::from(100);
    assert_ready!(l.poll_acquire());
    assert_eq!(l.take(), 100);
    l.release(); // should panic
}

#[test]
fn transfer_lease() {
    let mut l = Lease::from(100);

    assert_ready!(l.poll_acquire());

    // We should be able to transfer the acquired lease
    let mut l2 = l.transfer();
    // And then use it as normal
    assert_eq!(&*l2, &100);
    assert_eq!(l2.take(), 100);
    l2.restore(99);

    // Dropping the transferred lease is okay since we returned the value
    drop(l2);

    // Once the transferred lease has been restored, we can acquire the lease again
    assert_ready!(l.poll_acquire());
    assert_eq!(l.take(), 99);
    l.restore(98);
}

#[test]
fn readiness() {
    let mut task = MockTask::new();

    let mut l = Lease::from(100);
    assert_ready!(l.poll_acquire());
    let mut l2 = l.transfer();

    // We can't now acquire the lease since it's already held in l2
    task.enter(|| {
        assert_not_ready!(l.poll_acquire());
    });

    // But once l2 restores the value, we can acquire it
    l2.restore(99);
    assert!(task.is_notified());
    assert_ready!(l.poll_acquire());
}
