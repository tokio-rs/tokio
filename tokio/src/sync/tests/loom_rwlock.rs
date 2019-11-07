use crate::sync::rwlock::*;

use loom::future::block_on;
use loom::thread;
use std::sync::Arc;

// Attempt to acquire an RwLock for reading that already has a writer
#[test]
fn read_contested() {
    loom::model(|| {
        let rwlock = Arc::new(RwLock::<u32>::new(0));

        let rwclone = rwlock.clone();
        thread::spawn(move || {
            block_on(async {
                let mut guard = rwclone.write().await;
                *guard += 5;
            });
        });
        let rwclone = rwlock.clone();
        thread::spawn(move || {
            let result = block_on(async {
                let guard = rwclone.read().await;
                *guard
            });
            assert_eq!(5, result);
        });
    });
}

// Attempt to acquire an RwLock for reading after a writer has requested access
#[test]
fn read_write_contested() {
    loom::model(|| {
        let rwlock = Arc::new(RwLock::<u32>::new(42));

        let rwclone = rwlock.clone();
        thread::spawn(move || {
            let result = block_on(async {
                let guard = rwclone.read().await;
                *guard
            });
            assert_eq!(42, result);
        });

        let rwclone2 = rwlock.clone();
        thread::spawn(move || {
            block_on(async {
                let mut guard = rwclone2.write().await;
                *guard += 1;
            })
        });

        let rwclone3 = rwlock.clone();
        thread::spawn(move || {
            let result = block_on(async {
                let guard = rwclone3.read().await;
                *guard
            });
            assert_eq!(43, result);
        });

        let result = block_on(rwlock.read());
        assert_eq!(*result, 42);
    });
}
