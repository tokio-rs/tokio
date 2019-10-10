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

mod small_slab;

#[test]
fn local_remove() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());

        let s = slab.clone();
        let t1 = thread::spawn(move || {
            let idx = s.insert(1).expect("insert");
            assert_eq!(s.get_guard(idx), Some(1));
            s.remove(idx);
            assert_eq!(s.get_guard(idx), None);
            let idx = s.insert(2).expect("insert");
            assert_eq!(s.get_guard(idx), Some(2));
            s.remove(idx);
            assert_eq!(s.get_guard(idx), None);
        });

        let s = slab.clone();
        let t2 = thread::spawn(move || {
            let idx = s.insert(3).expect("insert");
            assert_eq!(s.get_guard(idx), Some(3));
            s.remove(idx);
            assert_eq!(s.get_guard(idx), None);
            let idx = s.insert(4).expect("insert");
            s.remove(idx);
            assert_eq!(s.get_guard(idx), None);
        });

        let s = slab;
        let idx1 = s.insert(5).expect("insert");
        assert_eq!(s.get_guard(idx1), Some(5));
        let idx2 = s.insert(6).expect("insert");
        assert_eq!(s.get_guard(idx2), Some(6));
        s.remove(idx1);
        assert_eq!(s.get_guard(idx1), None);
        assert_eq!(s.get_guard(idx2), Some(6));
        s.remove(idx2);
        assert_eq!(s.get_guard(idx2), None);

        t1.join().expect("thread 1 should not panic");
        t2.join().expect("thread 2 should not panic");
    });
}

#[test]
fn remove_remote() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());

        let idx1 = slab.insert(1).expect("insert");
        assert_eq!(slab.get_guard(idx1), Some(1));

        let idx2 = slab.insert(2).expect("insert");
        assert_eq!(slab.get_guard(idx2), Some(2));

        let idx3 = slab.insert(3).expect("insert");
        assert_eq!(slab.get_guard(idx3), Some(3));

        let s = slab.clone();
        let t1 = thread::spawn(move || {
            assert_eq!(s.get_guard(idx2), Some(2));
            s.remove(idx2);
            assert_eq!(s.get_guard(idx2), None);
        });

        let s = slab.clone();
        let t2 = thread::spawn(move || {
            assert_eq!(s.get_guard(idx3), Some(3));
            s.remove(idx3);
            assert_eq!(s.get_guard(idx3), None);
        });

        t1.join().expect("thread 1 should not panic");
        t2.join().expect("thread 2 should not panic");

        assert_eq!(slab.get_guard(idx1), Some(1));
        assert_eq!(slab.get_guard(idx2), None);
        assert_eq!(slab.get_guard(idx3), None);
    });
}

#[test]
fn concurrent_insert_remove() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());
        let pair = Arc::new((Mutex::new(None), Condvar::new()));

        let slab2 = slab.clone();
        let pair2 = pair.clone();
        let remover = thread::spawn(move || {
            let (lock, cvar) = &*pair2;
            for i in 0..2 {
                println!("--- remover i={} ---", i);
                let mut next = lock.lock().unwrap();
                while next.is_none() {
                    next = cvar.wait(next).unwrap();
                }
                let key = next.take().unwrap();
                slab2.remove(key);
                assert!(slab2.get_guard(key).is_none());
                cvar.notify_one();
            }
        });

        let (lock, cvar) = &*pair;
        for i in 0..2 {
            println!("--- inserter i={} ---", i);
            let key = slab.insert(i).expect("insert");

            let mut next = lock.lock().unwrap();
            *next = Some(key);
            cvar.notify_one();

            // Wait for the item to be removed.
            while next.is_some() {
                next = cvar.wait(next).unwrap();
            }

            assert!(slab.get_guard(key).is_none());
        }

        remover.join().unwrap();
    })
}

#[test]
fn unique_iter() {
    loom::model(|| {
        let mut slab = std::sync::Arc::new(Slab::new());

        let s = slab.clone();
        let t1 = thread::spawn(move || {
            s.insert(1).expect("insert");
            s.insert(2).expect("insert");
        });

        let s = slab.clone();
        let t2 = thread::spawn(move || {
            s.insert(3).expect("insert");
            s.insert(4).expect("insert");
        });

        t1.join().expect("thread 1 should not panic");
        t2.join().expect("thread 2 should not panic");

        let slab = std::sync::Arc::get_mut(&mut slab).expect("other arcs should be dropped");
        let items: Vec<_> = slab
            .unique_iter()
            .map(|i| i.aba_guard.load(Ordering::Acquire))
            .collect();
        assert!(items.contains(&1), "items: {:?}", items);
        assert!(items.contains(&2), "items: {:?}", items);
        assert!(items.contains(&3), "items: {:?}", items);
        assert!(items.contains(&4), "items: {:?}", items);
    });
}
