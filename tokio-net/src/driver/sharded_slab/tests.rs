use super::Slab;
use crate::sync::{Arc, Condvar, Mutex};
use loom::thread;

mod idx {
    use super::{
        cfg,
        page::{self, slot},
        Pack, Tid,
    };
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn tid_roundtrips(tid in 0usize..Tid::<cfg::DefaultConfig>::BITS) {
            let tid = Tid::<cfg::DefaultConfig>::from_usize(tid);
            let packed = tid.pack(0);
            assert_eq!(tid, Tid::from_packed(packed));
        }

        #[test]
        fn idx_roundtrips(
            tid in 0usize..Tid::<cfg::DefaultConfig>::BITS,
            gen in 0usize..slot::Generation::<cfg::DefaultConfig>::BITS,
            addr in 0usize..page::Addr::<cfg::DefaultConfig>::BITS,
        ) {
            let tid = Tid::<cfg::DefaultConfig>::from_usize(tid);
            let gen = slot::Generation::<cfg::DefaultConfig>::from_usize(gen);
            let addr = page::Addr::<cfg::DefaultConfig>::from_usize(addr);
            let packed = tid.pack(gen.pack(addr.pack(0)));
            assert_eq!(addr, page::Addr::from_packed(packed));
            assert_eq!(gen, slot::Generation::from_packed(packed));
            assert_eq!(tid, Tid::from_packed(packed));
        }
    }
}

struct TinyConfig;

impl super::Config for TinyConfig {
    const INITIAL_PAGE_SIZE: usize = 4;
}

#[test]
fn local_remove() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());

        let s = slab.clone();
        let t1 = thread::spawn(move || {
            let idx = s.insert(1).expect("insert");
            assert_eq!(s.get(idx), Some(&1));
            assert_eq!(s.remove(idx), Some(1));
            assert_eq!(s.get(idx), None);
            let idx = s.insert(2).expect("insert");
            assert_eq!(s.get(idx), Some(&2));
            assert_eq!(s.remove(idx), Some(2));
            assert_eq!(s.get(idx), None);
        });

        let s = slab.clone();
        let t2 = thread::spawn(move || {
            let idx = s.insert(3).expect("insert");
            assert_eq!(s.get(idx), Some(&3));
            assert_eq!(s.remove(idx), Some(3));
            assert_eq!(s.get(idx), None);
            let idx = s.insert(4).expect("insert");
            assert_eq!(s.get(idx), Some(&4));
            assert_eq!(s.remove(idx), Some(4));
            assert_eq!(s.get(idx), None);
        });

        let s = slab;
        let idx1 = s.insert(5).expect("insert");
        assert_eq!(s.get(idx1), Some(&5));
        let idx2 = s.insert(6).expect("insert");
        assert_eq!(s.get(idx2), Some(&6));
        assert_eq!(s.remove(idx1), Some(5));
        assert_eq!(s.get(idx1), None);
        assert_eq!(s.get(idx2), Some(&6));
        assert_eq!(s.remove(idx2), Some(6));
        assert_eq!(s.get(idx2), None);

        t1.join().expect("thread 1 should not panic");
        t2.join().expect("thread 2 should not panic");
    });
}

#[test]
fn remove_remote() {
    loom::model(|| {
        let slab = Arc::new(Slab::new());

        let idx1 = slab.insert(1).expect("insert");
        assert_eq!(slab.get(idx1), Some(&1));

        let idx2 = slab.insert(2).expect("insert");
        assert_eq!(slab.get(idx2), Some(&2));

        let idx3 = slab.insert(3).expect("insert");
        assert_eq!(slab.get(idx3), Some(&3));

        let s = slab.clone();
        let t1 = thread::spawn(move || {
            assert_eq!(s.get(idx2), Some(&2));
            assert_eq!(s.remove(idx2), Some(2));
        });

        let s = slab.clone();
        let t2 = thread::spawn(move || {
            assert_eq!(s.get(idx3), Some(&3));
            assert_eq!(s.remove(idx3), Some(3));
        });

        t1.join().expect("thread 1 should not panic");
        t2.join().expect("thread 2 should not panic");

        assert_eq!(slab.get(idx1), Some(&1));
        assert_eq!(slab.get(idx2), None);
        assert_eq!(slab.get(idx3), None);
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
                assert_eq!(slab2.remove(key), Some(i));
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

            assert!(slab.get(key).is_none());
        }

        remover.join().unwrap();
    })
}

#[test]
fn remove_remote_and_reuse() {
    loom::model(|| {
        let slab = Arc::new(Slab::new_with_config::<TinyConfig>());

        let idx1 = slab.insert(1).expect("insert");
        let idx2 = slab.insert(2).expect("insert");
        let idx3 = slab.insert(3).expect("insert");
        let idx4 = slab.insert(4).expect("insert");

        assert_eq!(slab.get(idx1), Some(&1), "slab: {:#?}", slab);
        assert_eq!(slab.get(idx2), Some(&2), "slab: {:#?}", slab);
        assert_eq!(slab.get(idx3), Some(&3), "slab: {:#?}", slab);
        assert_eq!(slab.get(idx4), Some(&4), "slab: {:#?}", slab);

        let s = slab.clone();
        let t1 = thread::spawn(move || {
            assert_eq!(s.remove(idx1), Some(1), "slab: {:#?}", s);
        });

        let s = slab.clone();
        let t2 = thread::spawn(move || {
            assert_eq!(s.remove(idx2), Some(2), "slab: {:#?}", s);
        });

        t1.join().expect("thread 1 should not panic");
        t2.join().expect("thread 2 should not panic");

        let idx1 = slab.insert(5).expect("insert");
        let idx2 = slab.insert(6).expect("insert");

        assert_eq!(slab.get(idx1), Some(&5), "slab: {:#?}", slab);
        assert_eq!(slab.get(idx2), Some(&6), "slab: {:#?}", slab);
        assert_eq!(slab.get(idx3), Some(&3), "slab: {:#?}", slab);
        assert_eq!(slab.get(idx4), Some(&4), "slab: {:#?}", slab);
    });
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
        let items: Vec<_> = slab.unique_iter().map(|&i| i).collect();
        assert!(items.contains(&1), "items: {:?}", items);
        assert!(items.contains(&2), "items: {:?}", items);
        assert!(items.contains(&3), "items: {:?}", items);
        assert!(items.contains(&4), "items: {:?}", items);
    });
}

// #[test]
// fn big() {
//     let mut model = loom::model::Builder::new();
//     model.max_branches = 4096;
//     model.check(|| {
//         let slab = Slab::new();
//         for i in 0..4096 {
//             let k = slab.insert(i).expect("insert");
//             assert_eq!(slab.get(k).expect("get"), &i);
//         }
//     })
// }

#[test]
fn custom_page_sz() {
    struct TinyConfig;

    impl super::Config for TinyConfig {
        const INITIAL_PAGE_SIZE: usize = 4;
    }

    let mut model = loom::model::Builder::new();
    model.max_branches = 20000;
    model.check(|| {
        let slab = Slab::new_with_config::<TinyConfig>();

        for i in 0..1024 {
            println!("{}", i);
            let k = slab.insert(i).expect("insert");
            assert_eq!(slab.get(k).expect("get"), &i, "slab: {:#?}", slab);
        }
    });
}
