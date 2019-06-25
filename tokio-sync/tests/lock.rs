#![deny(warnings, rust_2018_idioms)]

use pin_utils::pin_mut;
use std::future::Future;
use tokio_sync::lock::Lock;
use tokio_test::task::MockTask;
use tokio_test::{assert_pending, assert_ready};

#[test]
fn straight_execution() {
    let mut task = MockTask::new();
    let mut l = Lock::new(100);

    // We can immediately acquire the lock and take the value
    task.enter(|cx| {
        let mut g = assert_ready!(l.poll_lock(cx));
        assert_eq!(&*g, &100);
        *g = 99;
        drop(g);

        let mut g = assert_ready!(l.poll_lock(cx));
        assert_eq!(&*g, &99);
        *g = 98;
        drop(g);

        let mut g = assert_ready!(l.poll_lock(cx));
        assert_eq!(&*g, &98);

        // We can continue to access the guard even if the lock is dropped
        drop(l);
        *g = 97;
        assert_eq!(&*g, &97);
    });
}

#[test]
fn readiness() {
    let mut t1 = MockTask::new();
    let mut t2 = MockTask::new();

    let mut l = Lock::new(100);

    let g = assert_ready!(t1.enter(|cx| l.poll_lock(cx)));

    // We can't now acquire the lease since it's already held in g
    assert_pending!(t2.enter(|cx| l.poll_lock(cx)));

    // But once g unlocks, we can acquire it
    drop(g);
    assert!(t2.is_woken());
    assert_ready!(t2.enter(|cx| l.poll_lock(cx)));
}

#[test]
fn lock() {
    let mut lock = Lock::new(false);

    let mut lock2 = lock.clone();
    std::thread::spawn(move || {
        let l = lock2.lock();
        pin_mut!(l);

        let mut task = MockTask::new();
        let mut g = task.enter(|cx| assert_ready!(l.poll(cx)));
        std::thread::sleep(std::time::Duration::from_millis(500));
        *g = true;
        drop(g);
    });

    std::thread::sleep(std::time::Duration::from_millis(50));
    let mut task = MockTask::new();
    let l = lock.lock();
    pin_mut!(l);

    task.enter(|cx| assert_pending!(l.poll(cx)));
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(task.is_woken());
}
