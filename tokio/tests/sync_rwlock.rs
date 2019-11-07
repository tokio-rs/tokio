#![warn(rust_2018_idioms)]

use std::sync::Arc;

use futures_util::future::FutureExt;
use futures_util::stream;
use futures_util::stream::StreamExt;

use tokio::sync::{Barrier, RwLock};
use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready};

// multiple reads should be Ready
#[test]
fn read_shared() {
    let rwlock = RwLock::new(100);

    let mut t1 = spawn(rwlock.read());
    assert_ready!(t1.poll());
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

// Acquire an RwLock nonexclusively by a single task
#[tokio::test]
async fn read_uncontested() {
    let rwlock = RwLock::new(100);
    let result = *rwlock.read().await;

    assert_eq!(result, 100);
}

// Acquire an uncontested RwLock in exclusive mode.  poll immediately returns
// Poll::Ready
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
#[tokio::test]
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
