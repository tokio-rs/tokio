use crate::io::driver::ScheduledIo;
use crate::util::slab::{Address, Slab};

use loom::sync::{Arc, Condvar, Mutex};
use loom::thread;

#[test]
fn local_remove() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());

        let s = slab.clone();
        let t1 = thread::spawn(move || {
            let idx = store_val(&s, 1);
            assert_eq!(get_val(&s, idx), Some(1));
            s.remove(idx);
            assert_eq!(get_val(&s, idx), None);
            let idx = store_val(&s, 2);
            assert_eq!(get_val(&s, idx), Some(2));
            s.remove(idx);
            assert_eq!(get_val(&s, idx), None);
        });

        let s = slab.clone();
        let t2 = thread::spawn(move || {
            let idx = store_val(&s, 3);
            assert_eq!(get_val(&s, idx), Some(3));
            s.remove(idx);
            assert_eq!(get_val(&s, idx), None);
            let idx = store_val(&s, 4);
            s.remove(idx);
            assert_eq!(get_val(&s, idx), None);
        });

        let s = slab;
        let idx1 = store_val(&s, 5);
        assert_eq!(get_val(&s, idx1), Some(5));
        let idx2 = store_val(&s, 6);
        assert_eq!(get_val(&s, idx2), Some(6));
        s.remove(idx1);
        assert_eq!(get_val(&s, idx1), None);
        assert_eq!(get_val(&s, idx2), Some(6));
        s.remove(idx2);
        assert_eq!(get_val(&s, idx2), None);

        t1.join().expect("thread 1 should not panic");
        t2.join().expect("thread 2 should not panic");
    });
}

#[test]
fn remove_remote() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());

        let idx1 = store_val(&slab, 1);
        assert_eq!(get_val(&slab, idx1), Some(1));

        let idx2 = store_val(&slab, 2);
        assert_eq!(get_val(&slab, idx2), Some(2));

        let idx3 = store_val(&slab, 3);
        assert_eq!(get_val(&slab, idx3), Some(3));

        let s = slab.clone();
        let t1 = thread::spawn(move || {
            assert_eq!(get_val(&s, idx2), Some(2));
            s.remove(idx2);
            assert_eq!(get_val(&s, idx2), None);
        });

        let s = slab.clone();
        let t2 = thread::spawn(move || {
            assert_eq!(get_val(&s, idx3), Some(3));
            s.remove(idx3);
            assert_eq!(get_val(&s, idx3), None);
        });

        t1.join().expect("thread 1 should not panic");
        t2.join().expect("thread 2 should not panic");

        assert_eq!(get_val(&slab, idx1), Some(1));
        assert_eq!(get_val(&slab, idx2), None);
        assert_eq!(get_val(&slab, idx3), None);
    });
}

#[test]
fn remove_remote_and_reuse() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());

        let idx1 = store_val(&slab, 1);
        let idx2 = store_val(&slab, 2);

        assert_eq!(get_val(&slab, idx1), Some(1));
        assert_eq!(get_val(&slab, idx2), Some(2));

        let s = slab.clone();
        let t1 = thread::spawn(move || {
            s.remove(idx1);
            let value = get_val(&s, idx1);

            // We may or may not see the new value yet, depending on when
            // this occurs, but we must either see the new value or `None`;
            // the old value has been removed!
            assert!(value == None || value == Some(3));
        });

        let idx3 = store_when_free(&slab, 3);
        t1.join().expect("thread 1 should not panic");

        assert_eq!(get_val(&slab, idx3), Some(3));
        assert_eq!(get_val(&slab, idx2), Some(2));
    });
}

#[test]
fn concurrent_alloc_remove() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());
        let pair = Arc::new((Mutex::new(None), Condvar::new()));

        let slab2 = slab.clone();
        let pair2 = pair.clone();
        let remover = thread::spawn(move || {
            let (lock, cvar) = &*pair2;
            for _ in 0..2 {
                let mut next = lock.lock().unwrap();
                while next.is_none() {
                    next = cvar.wait(next).unwrap();
                }
                let key = next.take().unwrap();
                slab2.remove(key);
                assert_eq!(get_val(&slab2, key), None);
                cvar.notify_one();
            }
        });

        let (lock, cvar) = &*pair;
        for i in 0..2 {
            let key = store_val(&slab, i);

            let mut next = lock.lock().unwrap();
            *next = Some(key);
            cvar.notify_one();

            // Wait for the item to be removed.
            while next.is_some() {
                next = cvar.wait(next).unwrap();
            }

            assert_eq!(get_val(&slab, key), None);
        }

        remover.join().unwrap();
    })
}

#[test]
fn concurrent_remove_remote_and_reuse() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());

        let idx1 = store_val(&slab, 1);
        let idx2 = store_val(&slab, 2);

        assert_eq!(get_val(&slab, idx1), Some(1));
        assert_eq!(get_val(&slab, idx2), Some(2));

        let s = slab.clone();
        let s2 = slab.clone();
        let t1 = thread::spawn(move || {
            s.remove(idx1);
        });

        let t2 = thread::spawn(move || {
            s2.remove(idx2);
        });

        let idx3 = store_when_free(&slab, 3);
        t1.join().expect("thread 1 should not panic");
        t2.join().expect("thread 1 should not panic");

        assert!(get_val(&slab, idx1).is_none());
        assert!(get_val(&slab, idx2).is_none());
        assert_eq!(get_val(&slab, idx3), Some(3));
    });
}

#[test]
fn alloc_remove_get() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());
        let pair = Arc::new((Mutex::new(None), Condvar::new()));

        let slab2 = slab.clone();
        let pair2 = pair.clone();
        let t1 = thread::spawn(move || {
            let slab = slab2;
            let (lock, cvar) = &*pair2;
            // allocate one entry just so that we have to use the final one for
            // all future allocations.
            let _key0 = store_val(&slab, 0);
            let key = store_val(&slab, 1);

            let mut next = lock.lock().unwrap();
            *next = Some(key);
            cvar.notify_one();
            // remove the second entry
            slab.remove(key);
            // store a new readiness at the same location (since the slab
            // already has an entry in slot 0)
            store_val(&slab, 2);
        });

        let (lock, cvar) = &*pair;
        // wait for the second entry to be stored...
        let mut next = lock.lock().unwrap();
        while next.is_none() {
            next = cvar.wait(next).unwrap();
        }
        let key = next.unwrap();

        // our generation will be stale when the second store occurs at that
        // index, we must not see the value of that store.
        let val = get_val(&slab, key);
        assert_ne!(val, Some(2), "generation must have advanced!");

        t1.join().unwrap();
    })
}

#[test]
fn alloc_remove_set() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());
        let pair = Arc::new((Mutex::new(None), Condvar::new()));

        let slab2 = slab.clone();
        let pair2 = pair.clone();
        let t1 = thread::spawn(move || {
            let slab = slab2;
            let (lock, cvar) = &*pair2;
            // allocate one entry just so that we have to use the final one for
            // all future allocations.
            let _key0 = store_val(&slab, 0);
            let key = store_val(&slab, 1);

            let mut next = lock.lock().unwrap();
            *next = Some(key);
            cvar.notify_one();

            slab.remove(key);
            // remove the old entry and insert a new one, with a new generation.
            let key2 = slab.alloc().expect("store key 2");
            // after the remove, we must not see the value written with the
            // stale index.
            assert_eq!(
                get_val(&slab, key),
                None,
                "stale set must no longer be visible"
            );
            assert_eq!(get_val(&slab, key2), Some(0));
            key2
        });

        let (lock, cvar) = &*pair;

        // wait for the second entry to be stored. the index we get from the
        // other thread may become stale after a write.
        let mut next = lock.lock().unwrap();
        while next.is_none() {
            next = cvar.wait(next).unwrap();
        }
        let key = next.unwrap();

        // try to write to the index with our generation
        slab.get(key).map(|val| val.set_readiness(key, |_| 2));

        let key2 = t1.join().unwrap();
        // after the remove, we must not see the value written with the
        // stale index either.
        assert_eq!(
            get_val(&slab, key),
            None,
            "stale set must no longer be visible"
        );
        assert_eq!(get_val(&slab, key2), Some(0));
    });
}

fn get_val(slab: &Arc<Slab<ScheduledIo>>, address: Address) -> Option<usize> {
    slab.get(address).and_then(|s| s.get_readiness(address))
}

fn store_val(slab: &Arc<Slab<ScheduledIo>>, readiness: usize) -> Address {
    let key = slab.alloc().expect("allocate slot");

    if let Some(slot) = slab.get(key) {
        slot.set_readiness(key, |_| readiness)
            .expect("generation should still be valid!");
    } else {
        panic!("slab did not contain a value for {:?}", key);
    }

    key
}

fn store_when_free(slab: &Arc<Slab<ScheduledIo>>, readiness: usize) -> Address {
    let key = loop {
        if let Some(key) = slab.alloc() {
            break key;
        }

        thread::yield_now();
    };

    if let Some(slot) = slab.get(key) {
        slot.set_readiness(key, |_| readiness)
            .expect("generation should still be valid!");
    } else {
        panic!("slab did not contain a value for {:?}", key);
    }

    key
}
