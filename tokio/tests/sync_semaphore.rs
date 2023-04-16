#![cfg(feature = "sync")]

#[cfg(tokio_wasm_not_wasi)]
use wasm_bindgen_test::wasm_bindgen_test as test;

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
#[cfg(feature = "full")]
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
#[cfg(feature = "full")]
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

#[test]
fn merge() {
    let sem = Arc::new(Semaphore::new(3));
    {
        let mut p1 = sem.try_acquire().unwrap();
        assert_eq!(sem.available_permits(), 2);
        let p2 = sem.try_acquire_many(2).unwrap();
        assert_eq!(sem.available_permits(), 0);
        p1.merge(p2);
        assert_eq!(sem.available_permits(), 0);
    }
    assert_eq!(sem.available_permits(), 3);
}

#[test]
#[cfg(not(tokio_wasm))] // No stack unwinding on wasm targets
#[should_panic]
fn merge_unrelated_permits() {
    let sem1 = Arc::new(Semaphore::new(3));
    let sem2 = Arc::new(Semaphore::new(3));
    let mut p1 = sem1.try_acquire().unwrap();
    let p2 = sem2.try_acquire().unwrap();
    p1.merge(p2);
}

#[tokio::test]
#[cfg(feature = "full")]
async fn stress_test() {
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

#[test]
fn add_max_amount_permits() {
    let s = tokio::sync::Semaphore::new(0);
    s.add_permits(tokio::sync::Semaphore::MAX_PERMITS);
    assert_eq!(s.available_permits(), tokio::sync::Semaphore::MAX_PERMITS);
}

#[cfg(not(tokio_wasm))] // wasm currently doesn't support unwinding
#[test]
#[should_panic]
fn add_more_than_max_amount_permits1() {
    let s = tokio::sync::Semaphore::new(1);
    s.add_permits(tokio::sync::Semaphore::MAX_PERMITS);
}

#[cfg(not(tokio_wasm))] // wasm currently doesn't support unwinding
#[test]
#[should_panic]
fn add_more_than_max_amount_permits2() {
    let s = Semaphore::new(Semaphore::MAX_PERMITS - 1);
    s.add_permits(1);
    s.add_permits(1);
}

#[cfg(not(tokio_wasm))] // wasm currently doesn't support unwinding
#[test]
#[should_panic]
fn panic_when_exceeds_maxpermits() {
    let _ = Semaphore::new(Semaphore::MAX_PERMITS + 1);
}

#[test]
fn no_panic_at_maxpermits() {
    let _ = Semaphore::new(Semaphore::MAX_PERMITS);
    let s = Semaphore::new(Semaphore::MAX_PERMITS - 1);
    s.add_permits(1);
}
