use crate::sync::rwlock::*;

use loom::future::block_on;
use loom::thread;
use std::sync::Arc;

#[test]
fn concurrent_write() {
    let mut b = loom::model::Builder::new();

    b.check(|| {
        let rwlock = Arc::new(RwLock::<u32>::new(0));

        let rwclone = rwlock.clone();
        let t1 = thread::spawn(move || {
            block_on(async {
                let mut guard = rwclone.write().await;
                *guard += 5;
            });
        });

        let rwclone = rwlock.clone();
        let t2 = thread::spawn(move || {
            block_on(async {
                let mut guard = rwclone.write().await;
                *guard += 5;
            });
        });

        t1.join().expect("thread 1 write should not panic");
        t2.join().expect("thread 2 write should not panic");
        //when all threads have finished the value on the lock should be 10
        let guard = block_on(rwlock.read());
        assert_eq!(10, *guard);
    });
}

fn spin_read<T>(lock: &RwLock<T>) -> RwLockReadGuard<T> {
    loop {
        if let Ok(r) = lock.try_read() {
            break r;
        }
    }
}

fn spin_write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<T> {
    loop {
        if let Ok(r) = lock.try_write() {
            break r;
        }
    }
}

#[test]
fn concurrent_spin_write() {
    let mut b = loom::model::Builder::new();

    b.check(|| {
        let rwlock = Arc::new(RwLock::<u32>::new(0));

        let rwclone = rwlock.clone();
        let t1 = thread::spawn(move || {
            let mut guard = spin_write(&rwclone);
            *guard += 5;
        });

        let rwclone = rwlock.clone();
        let t2 = thread::spawn(move || {
            let mut guard = spin_write(&rwclone);
            *guard += 5;
        });

        t1.join().expect("thread 1 write should not panic");
        t2.join().expect("thread 2 write should not panic");
        // when all threads have finished the value on the lock should be 10
        let guard = spin_read(&rwlock);
        assert_eq!(10, *guard);
    });
}

#[test]
fn concurrent_read_write() {
    let mut b = loom::model::Builder::new();

    b.check(|| {
        let rwlock = Arc::new(RwLock::<u32>::new(0));

        let rwclone = rwlock.clone();
        let t1 = thread::spawn(move || {
            block_on(async {
                let mut guard = rwclone.write().await;
                *guard += 5;
            });
        });

        let rwclone = rwlock.clone();
        let t2 = thread::spawn(move || {
            block_on(async {
                let mut guard = rwclone.write().await;
                *guard += 5;
            });
        });

        let rwclone = rwlock.clone();
        let t3 = thread::spawn(move || {
            block_on(async {
                let guard = rwclone.read().await;
                //at this state the value on the lock may either be 0, 5, or 10
                assert!(*guard == 0 || *guard == 5 || *guard == 10);
            });
        });

        t1.join().expect("thread 1 write should not panic");
        t2.join().expect("thread 2 write should not panic");
        t3.join().expect("thread 3 read should not panic");

        let guard = block_on(rwlock.read());
        //when all threads have finished the value on the lock should be 10
        assert_eq!(10, *guard);
    });
}

#[test]
fn concurrent_spin_read_write() {
    let mut b = loom::model::Builder::new();

    b.check(|| {
        let rwlock = Arc::new(RwLock::<u32>::new(0));

        let rwclone = rwlock.clone();
        let t1 = thread::spawn(move || {
            let mut guard = spin_write(&rwclone);
            *guard += 5;
        });

        let rwclone = rwlock.clone();
        let t2 = thread::spawn(move || {
            let mut guard = spin_write(&rwclone);
            *guard += 5;
        });

        let rwclone = rwlock.clone();
        let t3 = thread::spawn(move || {
            let guard = spin_read(&rwclone);
            // at this state the value on the lock may either be 0, 5, or 10
            assert!(*guard == 0 || *guard == 5 || *guard == 10);
        });

        t1.join().expect("thread 1 write should not panic");
        t2.join().expect("thread 2 write should not panic");
        t3.join().expect("thread 3 read should not panic");

        let guard = spin_read(&rwlock);
        // when all threads have finished the value on the lock should be 10
        assert_eq!(10, *guard);
    });
}
