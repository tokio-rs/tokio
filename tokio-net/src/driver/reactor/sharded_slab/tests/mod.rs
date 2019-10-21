use super::Slab;
use crate::sync::atomic::Ordering;
use loom::sync::{Arc, Condvar, Mutex};
use loom::thread;

mod idx {
    use super::super::{page, Pack, Tid};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn tid_roundtrips(tid in 0usize..Tid::BITS) {
            let tid = Tid::from_usize(tid);
            let packed = tid.pack(0);
            assert_eq!(tid, Tid::from_packed(packed));
        }

        #[test]
        fn idx_roundtrips(
            tid in 0usize..Tid::BITS,
            addr in 0usize..page::Addr::BITS,
        ) {
            let tid = Tid::from_usize(tid);
            let addr = page::Addr::from_usize(addr);
            let packed = tid.pack(addr.pack(0));
            assert_eq!(addr, page::Addr::from_packed(packed));
            assert_eq!(tid, Tid::from_packed(packed));
        }
    }
}

fn store_val(slab: &Arc<Slab>, readiness: usize) -> usize {
    println!("store: {}", readiness);
    let key = slab.alloc().expect("allocate slot");
    if let Some(slot) = slab.get(key) {
        slot.readiness.store(readiness, Ordering::Release);
    } else {
        panic!("slab did not contain a value for {:#x}", key);
    }
    key
}

fn get_val(slab: &Arc<Slab>, key: usize) -> Option<usize> {
    slab.get(key).map(|s| s.readiness.load(Ordering::Acquire))
}

mod single_shard;
mod small_slab;

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
fn concurrent_alloc_remove() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());
        let pair = Arc::new((Mutex::new(None), Condvar::new()));

        let slab2 = slab.clone();
        let pair2 = pair.clone();
        let remover = thread::spawn(move || {
            let (lock, cvar) = &*pair2;
            for i in 0..2 {
                test_println!("--- remover i={} ---", i);
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
            test_println!("--- allocer i={} ---", i);
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
fn unique_iter() {
    loom::model(|| {
        let mut slab = Arc::new(Slab::new());

        let s = slab.clone();
        let t1 = thread::spawn(move || {
            store_val(&s, 1);
            store_val(&s, 2);
        });

        let s = slab.clone();
        let t2 = thread::spawn(move || {
            store_val(&s, 3);
            store_val(&s, 4);
        });

        t1.join().expect("thread 1 should not panic");
        t2.join().expect("thread 2 should not panic");

        let slab = Arc::get_mut(&mut slab).expect("other arcs should be dropped");
        let items: Vec<_> = slab
            .unique_iter()
            .map(|i| i.readiness.load(Ordering::Acquire))
            .collect();
        assert!(items.contains(&1), "items: {:?}", items);
        assert!(items.contains(&2), "items: {:?}", items);
        assert!(items.contains(&3), "items: {:?}", items);
        assert!(items.contains(&4), "items: {:?}", items);
    });
}
