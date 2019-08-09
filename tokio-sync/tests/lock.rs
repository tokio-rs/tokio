#![warn(rust_2018_idioms)]

use tokio_sync::Lock;
use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready};

#[test]
fn straight_execution() {
    let mut l = Lock::new(100);

    {
        let mut t = spawn(l.lock());
        let mut g = assert_ready!(t.poll());
        assert_eq!(&*g, &100);
        *g = 99;
    }
    {
        let mut t = spawn(l.lock());
        let mut g = assert_ready!(t.poll());
        assert_eq!(&*g, &99);
        *g = 98;
    }
    {
        let mut t = spawn(l.lock());
        let mut g = assert_ready!(t.poll());
        assert_eq!(&*g, &98);

        // We can continue to access the guard even if the lock is dropped
        drop(t);
        drop(l);
        *g = 97;
        assert_eq!(&*g, &97);
    }
}

#[test]
fn readiness() {
    let mut l1 = Lock::new(100);
    let mut l2 = l1.clone();
    let mut t1 = spawn(l1.lock());
    let mut t2 = spawn(l2.lock());

    let g = assert_ready!(t1.poll());

    // We can't now acquire the lease since it's already held in g
    assert_pending!(t2.poll());

    // But once g unlocks, we can acquire it
    drop(g);
    assert!(t2.is_woken());
    assert_ready!(t2.poll());
}

/*
#[test]
#[ignore]
fn lock() {
    let mut lock = Lock::new(false);

    let mut lock2 = lock.clone();
    std::thread::spawn(move || {
        let l = lock2.lock();
        pin_mut!(l);

        let mut task = MockTask::new();
        let mut g = assert_ready!(task.poll(&mut l));
        std::thread::sleep(std::time::Duration::from_millis(500));
        *g = true;
        drop(g);
    });

    std::thread::sleep(std::time::Duration::from_millis(50));
    let mut task = MockTask::new();
    let l = lock.lock();
    pin_mut!(l);

    assert_pending!(task.poll(&mut l));

    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(task.is_woken());
    let result = assert_ready!(task.poll(&mut l));
    assert!(*result);
}
*/
