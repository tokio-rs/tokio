use super::test_util;
use loom::sync::{Arc, Condvar, Mutex};
use loom::thread;

use pack::{Pack, WIDTH};
use sharded_slab::Shard;
use sharded_slab::Slab;
use tid::Tid;

// Overridden for tests
const INITIAL_PAGE_SIZE: usize = 2;
const MAX_PAGES: usize = 1;

// Constants not overridden
#[cfg(target_pointer_width = "64")]
const MAX_THREADS: usize = 4096;
#[cfg(target_pointer_width = "32")]
const MAX_THREADS: usize = 2048;
const RESERVED_BITS: usize = 5;

#[path = "../../page/mod.rs"]
#[allow(dead_code)]
mod page;

#[path = "../../pack.rs"]
#[allow(dead_code)]
mod pack;

#[path = "../../iter.rs"]
#[allow(dead_code)]
mod iter;

#[path = "../../sharded_slab.rs"]
#[allow(dead_code)]
mod sharded_slab;

#[path = "../../tid.rs"]
#[allow(dead_code)]
mod tid;

fn store_val(slab: &Arc<Slab>, readiness: usize) -> usize {
    println!("store: {}", readiness);
    let key = slab.alloc().expect("allocate slot");
    if let Some(slot) = slab.get(key) {
        slot.set_readiness(key, |_| readiness)
            .expect("generation should still be valid!");
    } else {
        panic!("slab did not contain a value for {:#x}", key);
    }
    key
}

fn get_val(slab: &Arc<Slab>, key: usize) -> Option<usize> {
    slab.get(key).and_then(|s| {
        let rdy = s.get_readiness(key);
        test_println!("--> got readiness {:?} with key {:#x}", rdy, key);
        rdy
    })
}

fn store_when_free(slab: &Arc<Slab>, readiness: usize) -> usize {
    test_println!("store: {}", readiness);
    let key = loop {
        if let Some(key) = slab.alloc() {
            break key;
        }
        test_println!("-> full; retry");
        thread::yield_now();
    };
    if let Some(slot) = slab.get(key) {
        slot.set_readiness(key, |_| readiness)
            .expect("generation should still be valid!");
    } else {
        panic!("slab did not contain a value for {:#x}", key);
    }
    key
}

#[test]
fn remove_remote_and_reuse() {
    let mut model = loom::model::Builder::new();
    model.max_branches = 100000;
    test_util::run_builder("remove_remote_and_reuse", model, || {
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
            // this occurs, but we must either  see the new value or `None`;
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
fn concurrent_remove_remote_and_reuse() {
    let mut model = loom::model::Builder::new();
    model.max_branches = 100000;
    // set a preemption bound, or else this will run for a *really* long time.
    model.preemption_bound = Some(2); // chosen arbitrarily.
    test_util::run_builder("concurrent_remove_remote_and_reuse", model, || {
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

mod single_shard {
    use super::sharded_slab::SingleShard;
    use super::*;

    fn store_val(slab: &Arc<SingleShard>, readiness: usize) -> usize {
        println!("store: {}", readiness);
        let key = slab.alloc().expect("allocate slot");
        if let Some(slot) = slab.get(key) {
            slot.set_readiness(key, |_| readiness)
                .expect("generation should still be valid!");
        } else {
            panic!("slab did not contain a value for {:#x}", key);
        }
        key
    }

    fn get_val(slab: &Arc<SingleShard>, key: usize) -> Option<usize> {
        slab.get(key).and_then(|s| {
            let rdy = s.get_readiness(key);
            test_println!("--> got readiness {:?} with key {:#x}", rdy, key);
            rdy
        })
    }

    fn store_when_free(slab: &Arc<SingleShard>, readiness: usize) -> usize {
        test_println!("store: {}", readiness);
        let key = loop {
            if let Some(key) = slab.alloc() {
                break key;
            }
            test_println!("-> full; retry");
            thread::yield_now();
        };
        if let Some(slot) = slab.get(key) {
            slot.set_readiness(key, |_| readiness)
                .expect("generation should still be valid!");
        } else {
            panic!("slab did not contain a value for {:#x}", key);
        }
        key
    }

    #[test]
    fn remove_remote_and_reuse() {
        let mut model = loom::model::Builder::new();
        model.max_branches = 100000;
        test_util::run_builder("single_shard::remove_remote_and_reuse", model, || {
            let slab = Arc::new(SingleShard::new());

            let idx1 = store_val(&slab, 1);
            let idx2 = store_val(&slab, 2);

            assert_eq!(get_val(&slab, idx1), Some(1));
            assert_eq!(get_val(&slab, idx2), Some(2));

            let s = slab.clone();
            let t1 = thread::spawn(move || {
                s.remove(idx1);
                let value = get_val(&s, idx1);

                // We may or may not see the new value yet, depending on when
                // this occurs, but we must either  see the new value or `None`;
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
    fn concurrent_remove_remote_and_reuse() {
        let mut model = loom::model::Builder::new();
        model.max_branches = 100000;
        // set a preemption bound, or else this will run for a *really* long time.
        model.preemption_bound = Some(2); // chosen arbitrarily.
        test_util::run_builder("single_shard::remove_remote_and_reuse", model, || {
            let slab = Arc::new(SingleShard::new());

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
        test_util::run_model("single_shard::alloc_remove_get", || {
            let slab = Arc::new(SingleShard::new());
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
        test_util::run_model("single_shard::alloc_remove_set", || {
            let slab = Arc::new(SingleShard::new());
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
        })
    }
}

#[test]
fn alloc_remove_get() {
    test_util::run_model("alloc_remove_get", || {
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
    test_util::run_model("alloc_remove_set", || {
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
    })
}

// #[test]
// fn custom_page_sz() {
//     let mut model = loom::model::Builder::new();
//     model.max_branches = 100000;
//     model.check(|| {
//         let slab = Arc::new(Slab::new());

//         for i in 0..1024 {
//             test_println!("{}", i);
//             let k = store_val(&slab, i);
//             assert_eq!(get_val(&slab, k), Some(i));
//         }
//     });
// }
