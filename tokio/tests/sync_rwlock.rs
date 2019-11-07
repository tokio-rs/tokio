#![warn(rust_2018_idioms)]

use std::sync::Arc;

use futures_util::future::FutureExt;
use futures_util::stream::StreamExt;
use futures_util::try_future::TryFutureExt;
use futures_util::{future, stream};

use tokio::sync::{oneshot, RwLock};
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

// Attempt to acquire an RwLock for reading that already has a writer
#[tokio::test]
async fn read_contested() {
    let rwlock = RwLock::<u32>::new(0);
    let (tx0, rx0) = oneshot::channel::<()>();

    let task1 = rwlock.write().then(move |mut guard| {
        *guard += 5;
        rx0.map_err(|_| {
            drop(guard);
        })
    });

    let task2 = rwlock.read().map(|guard| *guard);
    let task3 = rwlock.read().map(|guard| *guard);
    let task4 = async { tx0.send(()) };

    let result = future::join4(task1, task2, task3, task4).await;

    assert_eq!(result, (Ok(()), 5, 5, Ok(())));
}

// Attempt to acquire an RwLock for reading after a writer has requested access
#[tokio::test]
async fn read_write_contested() {
    let rwlock = RwLock::<u32>::new(42);

    let task1 = rwlock.read().map(move |guard| *guard);
    let task2 = rwlock.write().map(|mut guard| *guard += 1);
    let task3 = rwlock.read().map(|guard| *guard);

    let result = future::join3(task1, task2, task3).await;
    assert_eq!(result, (42, (), 43));

    let g = rwlock.read().await;
    assert_eq!(*g, 43);
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
    let rwlock = Arc::new(RwLock::<u32>::new(0));

    let fut1 = stream::iter(0..1000).for_each(|_| {
        async {
            let rwlock_clone = rwlock.clone();
            let mut guard = rwlock_clone.write().await;
            *guard += 2;
        }
    });

    let fut2 = stream::iter(0..1000).for_each(|_| {
        async {
            let rwlock_clone = rwlock.clone();
            let mut guard = rwlock_clone.write().await;
            *guard += 3;
        }
    });

    let fut3 = stream::iter(0..1000).for_each(|_| {
        async {
            let rwlock_clone = rwlock.clone();
            let mut guard = rwlock_clone.write().await;
            *guard += 5;
        }
    });

    let fut4 = stream::iter(0..1000).for_each(|_| {
        async {
            let rwlock_clone = rwlock.clone();
            let mut guard = rwlock_clone.write().await;
            *guard += 7;
        }
    });

    future::join4(fut1, fut2, fut3, fut4).await;
    let g = rwlock.read().await;
    assert_eq!(*g, 17_000);
}
