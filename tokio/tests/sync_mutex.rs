#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::sync::Mutex;
use tokio::time::{interval, timeout};
use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready};

use std::sync::Arc;
use std::time::Duration;

#[test]
fn straight_execution() {
    let l = Mutex::new(100);

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
        let g = assert_ready!(t.poll());
        assert_eq!(&*g, &98);
    }
}

#[test]
fn readiness() {
    let l1 = Arc::new(Mutex::new(100));
    let l2 = Arc::clone(&l1);
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
    let mut lock = Mutex::new(false);

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

#[tokio::test]
/// Ensure a mutex is unlocked if a future holding the lock
/// is aborted prematurely.
async fn aborted_future_1() {
    let m1: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    {
        let m2 = m1.clone();
        // Try to lock mutex in a future that is aborted prematurely
        timeout(Duration::from_millis(1u64), async move {
            let mut iv = interval(Duration::from_millis(1000));
            m2.lock().await;
            iv.tick().await;
            iv.tick().await;
        })
        .await
        .unwrap_err();
    }
    // This should succeed as there is no lock left for the mutex.
    timeout(Duration::from_millis(1u64), async move {
        m1.lock().await;
    })
    .await
    .expect("Mutex is locked");
}

#[tokio::test]
/// This test is similar to `aborted_future_1` but this time the
/// aborted future is waiting for the lock.
async fn aborted_future_2() {
    let m1: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    {
        // Lock mutex
        let _lock = m1.lock().await;
        {
            let m2 = m1.clone();
            // Try to lock mutex in a future that is aborted prematurely
            timeout(Duration::from_millis(1u64), async move {
                m2.lock().await;
            })
            .await
            .unwrap_err();
        }
    }
    // This should succeed as there is no lock left for the mutex.
    timeout(Duration::from_millis(1u64), async move {
        m1.lock().await;
    })
    .await
    .expect("Mutex is locked");
}

#[test]
fn try_lock() {
    let m: Mutex<usize> = Mutex::new(0);
    {
        let g1 = m.try_lock();
        assert_eq!(g1.is_ok(), true);
        let g2 = m.try_lock();
        assert_eq!(g2.is_ok(), false);
    }
    let g3 = m.try_lock();
    assert_eq!(g3.is_ok(), true);
}

#[tokio::test]
async fn debug_format() {
    let s = "debug";
    let m = Mutex::new(s.to_string());
    assert_eq!(format!("{:?}", s), format!("{:?}", m.lock().await));
}
