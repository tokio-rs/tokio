#![cfg(feature = "full")]

use std::sync::Arc;
use tokio::sync::semaphore::Semaphore;

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

#[test]
fn try_acquire_closed() {
    let sem = Arc::new(Semaphore::new(1));
    sem.close();
    assert!(sem.try_acquire().is_err());
}

#[tokio::test]
async fn acquire_closed() {
    let sem = Arc::new(Semaphore::new(1));
    let p1 = sem.try_acquire();
    let sem_clone = sem.clone();
    let j = tokio::spawn(async move { sem_clone.acquire().await.is_ok() });
    sem.close();
    assert!(!j.await.unwrap());
    assert!(sem.acquire().await.is_err());
    // dropping the permit should not make it possible to
    // grab a permit from a closed permit
    drop(p1);
    assert!(sem.acquire().await.is_err());
}

#[tokio::test]
async fn acquire() {
    let sem = Arc::new(Semaphore::new(1));
    let p1 = sem.try_acquire().unwrap();
    let sem_clone = sem.clone();
    let j = tokio::spawn(async move {
        let _p2 = sem_clone.acquire().await.unwrap();
    });
    drop(p1);
    j.await.unwrap();
}

#[tokio::test]
async fn add_permits() {
    let sem = Arc::new(Semaphore::new(0));
    let sem_clone = sem.clone();
    let j = tokio::spawn(async move {
        let _p2 = sem_clone.acquire().await.unwrap();
    });
    sem.add_permits(1);
    j.await.unwrap();
}

#[tokio::test]
async fn remove_permits() {
    let sem = Arc::new(Semaphore::new(5));
    sem.remove_permits(3).await.unwrap();
    assert_eq!(sem.available_permits(), 2);
}

#[test]
fn try_remove_permits() {
    let sem = Arc::new(Semaphore::new(3));
    let p = sem.try_acquire().unwrap();
    assert_eq!(sem.try_remove_permits(3), 2);
    drop(p);
    assert_eq!(sem.try_remove_permits(3), 1);
    assert_eq!(sem.try_remove_permits(3), 0);
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
            let _p = sem_clone.acquire().await.unwrap();
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
