#![cfg(feature = "full")]

use std::sync::Arc;
use tokio::sync::Semaphore;

#[test]
fn no_permits() {
    // this should not panic
    Semaphore::new(0);
}

#[test]
fn try_acquire() {
    let sem = Semaphore::new(1);
    {
        let p1 = sem.try_acquire();
        assert!(p1.is_ok());
        let p2 = sem.try_acquire();
        assert!(p2.is_err());
    }
    let p3 = sem.try_acquire();
    assert!(p3.is_ok());
}

#[tokio::test]
async fn acquire() {
    let sem = Arc::new(Semaphore::new(1));
    let p1 = sem.try_acquire().unwrap();
    let sem_clone = sem.clone();
    let j = tokio::spawn(async move {
        let _p2 = sem_clone.acquire().await;
    });
    drop(p1);
    j.await.unwrap();
}

#[tokio::test]
async fn add_permits() {
    let sem = Arc::new(Semaphore::new(0));
    let sem_clone = sem.clone();
    let j = tokio::spawn(async move {
        let _p2 = sem_clone.acquire().await;
    });
    sem.add_permits(1);
    j.await.unwrap();
}

#[test]
fn forget() {
    let sem = Arc::new(Semaphore::new(1));
    {
        let p = sem.try_acquire().unwrap();
        assert_eq!(sem.available_permits(), 0);
        p.forget();
        assert_eq!(sem.available_permits(), 0);
    }
    assert_eq!(sem.available_permits(), 0);
    assert!(sem.try_acquire().is_err());
}

#[tokio::test]
async fn stresstest() {
    let sem = Arc::new(Semaphore::new(5));
    let mut join_handles = Vec::new();
    for _ in 0..1000 {
        let sem_clone = sem.clone();
        join_handles.push(tokio::spawn(async move {
            let _p = sem_clone.acquire().await;
        }));
    }
    for j in join_handles {
        j.await.unwrap();
    }
    // there should be exactly 5 semaphores available now
    let _p1 = sem.try_acquire().unwrap();
    let _p2 = sem.try_acquire().unwrap();
    let _p3 = sem.try_acquire().unwrap();
    let _p4 = sem.try_acquire().unwrap();
    let _p5 = sem.try_acquire().unwrap();
    assert!(sem.try_acquire().is_err());
}
