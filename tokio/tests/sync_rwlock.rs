#![warn(rust_2018_idioms)]

use std::sync::Arc;
use std::task::Poll;

use futures::future::FutureExt;
use futures::stream;
use futures::stream::StreamExt;

use tokio::sync::{Barrier, RwLock};
use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready};

#[test]
fn into_inner() {
    let rwlock = RwLock::new(42);
    assert_eq!(rwlock.into_inner(), 42);
}

// multiple reads should be Ready
#[test]
fn read_shared() {
    let rwlock = RwLock::new(100);

    let mut t1 = spawn(rwlock.read());
    let _g1 = assert_ready!(t1.poll());
    let mut t2 = spawn(rwlock.read());
    assert_ready!(t2.poll());
}

// When there is an active shared owner, exclusive access should not be possible
#[test]
fn write_shared_pending() {
    let rwlock = RwLock::new(100);
    let mut t1 = spawn(rwlock.read());

    let _g1 = assert_ready!(t1.poll());
    let mut t2 = spawn(rwlock.write());
    assert_pending!(t2.poll());
}

// When there is an active exclusive owner, subsequent exclusive access should not be possible
#[test]
fn read_exclusive_pending() {
    let rwlock = RwLock::new(100);
    let mut t1 = spawn(rwlock.write());

    let _g1 = assert_ready!(t1.poll());
    let mut t2 = spawn(rwlock.read());
    assert_pending!(t2.poll());
}

// If the max shared access is reached and subsquent shared access is pending
// should be made available when one of the shared acesses is dropped
#[test]
fn exhaust_reading() {
    let rwlock = RwLock::with_max_readers(100, 1024);
    let mut reads = Vec::new();
    loop {
        let mut t = spawn(rwlock.read());
        match t.poll() {
            Poll::Ready(guard) => reads.push(guard),
            Poll::Pending => break,
        }
    }

    let mut t1 = spawn(rwlock.read());
    assert_pending!(t1.poll());
    let g2 = reads.pop().unwrap();
    drop(g2);
    assert!(t1.is_woken());
    assert_ready!(t1.poll());
}

// When there is an active exclusive owner, subsequent exclusive access should not be possible
#[test]
fn write_exclusive_pending() {
    let rwlock = RwLock::new(100);
    let mut t1 = spawn(rwlock.write());

    let _g1 = assert_ready!(t1.poll());
    let mut t2 = spawn(rwlock.write());
    assert_pending!(t2.poll());
}

// When there is an active shared owner, exclusive access should be possible after shared is dropped
#[test]
fn write_shared_drop() {
    let rwlock = RwLock::new(100);
    let mut t1 = spawn(rwlock.read());

    let g1 = assert_ready!(t1.poll());
    let mut t2 = spawn(rwlock.write());
    assert_pending!(t2.poll());
    drop(g1);
    assert!(t2.is_woken());
    assert_ready!(t2.poll());
}

// when there is an active shared owner, and exclusive access is triggered,
// subsequent shared access should not be possible as write gathers all the available semaphore permits
#[test]
fn write_read_shared_pending() {
    let rwlock = RwLock::new(100);
    let mut t1 = spawn(rwlock.read());
    let _g1 = assert_ready!(t1.poll());

    let mut t2 = spawn(rwlock.read());
    assert_ready!(t2.poll());

    let mut t3 = spawn(rwlock.write());
    assert_pending!(t3.poll());

    let mut t4 = spawn(rwlock.read());
    assert_pending!(t4.poll());
}

// when there is an active shared owner, and exclusive access is triggered,
// reading should be possible after pending exclusive access is dropped
#[test]
fn write_read_shared_drop_pending() {
    let rwlock = RwLock::new(100);
    let mut t1 = spawn(rwlock.read());
    let _g1 = assert_ready!(t1.poll());

    let mut t2 = spawn(rwlock.write());
    assert_pending!(t2.poll());

    let mut t3 = spawn(rwlock.read());
    assert_pending!(t3.poll());
    drop(t2);

    assert!(t3.is_woken());
    assert_ready!(t3.poll());
}

// Acquire an RwLock nonexclusively by a single task
#[tokio::test]
async fn read_uncontested() {
    let rwlock = RwLock::new(100);
    let result = *rwlock.read().await;

    assert_eq!(result, 100);
}

// Acquire an uncontested RwLock in exclusive mode
#[tokio::test]
async fn write_uncontested() {
    let rwlock = RwLock::new(100);
    let mut result = rwlock.write().await;
    *result += 50;
    assert_eq!(*result, 150);
}

// RwLocks should be acquired in the order that their Futures are waited upon.
#[tokio::test]
async fn write_order() {
    let rwlock = RwLock::<Vec<u32>>::new(vec![]);
    let fut2 = rwlock.write().map(|mut guard| guard.push(2));
    let fut1 = rwlock.write().map(|mut guard| guard.push(1));
    fut1.await;
    fut2.await;

    let g = rwlock.read().await;
    assert_eq!(*g, vec![1, 2]);
}

// A single RwLock is contested by tasks in multiple threads
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn multithreaded() {
    let barrier = Arc::new(Barrier::new(5));
    let rwlock = Arc::new(RwLock::<u32>::new(0));
    let rwclone1 = rwlock.clone();
    let rwclone2 = rwlock.clone();
    let rwclone3 = rwlock.clone();
    let rwclone4 = rwlock.clone();

    let b1 = barrier.clone();
    tokio::spawn(async move {
        stream::iter(0..1000)
            .for_each(move |_| {
                let rwlock = rwclone1.clone();
                async move {
                    let mut guard = rwlock.write().await;
                    *guard += 2;
                }
            })
            .await;
        b1.wait().await;
    });

    let b2 = barrier.clone();
    tokio::spawn(async move {
        stream::iter(0..1000)
            .for_each(move |_| {
                let rwlock = rwclone2.clone();
                async move {
                    let mut guard = rwlock.write().await;
                    *guard += 3;
                }
            })
            .await;
        b2.wait().await;
    });

    let b3 = barrier.clone();
    tokio::spawn(async move {
        stream::iter(0..1000)
            .for_each(move |_| {
                let rwlock = rwclone3.clone();
                async move {
                    let mut guard = rwlock.write().await;
                    *guard += 5;
                }
            })
            .await;
        b3.wait().await;
    });

    let b4 = barrier.clone();
    tokio::spawn(async move {
        stream::iter(0..1000)
            .for_each(move |_| {
                let rwlock = rwclone4.clone();
                async move {
                    let mut guard = rwlock.write().await;
                    *guard += 7;
                }
            })
            .await;
        b4.wait().await;
    });

    barrier.wait().await;
    let g = rwlock.read().await;
    assert_eq!(*g, 17_000);
}

#[tokio::test]
async fn try_write() {
    let lock = RwLock::new(0);
    let read_guard = lock.read().await;
    assert!(lock.try_write().is_err());
    drop(read_guard);
    assert!(lock.try_write().is_ok());
}

#[test]
fn try_read_try_write() {
    let lock: RwLock<usize> = RwLock::new(15);

    {
        let rg1 = lock.try_read().unwrap();
        assert_eq!(*rg1, 15);

        assert!(lock.try_write().is_err());

        let rg2 = lock.try_read().unwrap();
        assert_eq!(*rg2, 15)
    }

    {
        let mut wg = lock.try_write().unwrap();
        *wg = 1515;

        assert!(lock.try_read().is_err())
    }

    assert_eq!(*lock.try_read().unwrap(), 1515);
}
