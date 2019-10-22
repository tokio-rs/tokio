use super::super::ScheduledIo;
use loom::sync::{atomic::Ordering, Arc};
use loom::thread;

use pack::{Pack, WIDTH};
use slab::Shard;
use slab::Slab;
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

#[path = "../page/mod.rs"]
#[allow(dead_code)]
mod page;

#[path = "../pack.rs"]
#[allow(dead_code)]
mod pack;

#[path = "../iter.rs"]
#[allow(dead_code)]
mod iter;

#[path = "../slab.rs"]
#[allow(dead_code)]
mod slab;

#[path = "../tid.rs"]
#[allow(dead_code)]
mod tid;

fn store_val(slab: &Arc<Slab>, readiness: usize) -> usize {
    test_println!("store: {}", readiness);
    let key = slab.alloc().expect("allocate slot");
    if let Some(slot) = slab.get(key) {
        slot.readiness.store(readiness, Ordering::Release);
    } else {
        panic!("slab did not contain a value for {:#x}", key);
    }
    key
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
        slot.readiness.store(readiness, Ordering::Release);
    } else {
        panic!("slab did not contain a value for {:#x}", key);
    }
    key
}

fn get_val(slab: &Arc<Slab>, key: usize) -> Option<usize> {
    slab.get(key).map(|s| s.readiness.load(Ordering::Acquire))
}

#[test]
fn remove_remote_and_reuse() {
    let mut model = loom::model::Builder::new();
    model.max_branches = 100000;
    model.check(|| {
        test_println!("\n --- iteration ---\n");
        let slab = Arc::new(Slab::new());

        let idx1 = store_val(&slab, 1);
        let idx2 = store_val(&slab, 2);

        assert_eq!(get_val(&slab, idx1), Some(1), "slab: {:#?}", slab);
        assert_eq!(get_val(&slab, idx2), Some(2), "slab: {:#?}", slab);

        let s = slab.clone();
        let t1 = thread::spawn(move || {
            s.remove(idx1);
            let value = get_val(&s, idx1);

            // We may or may not see the new value yet, depending on when
            // this occurs, but we must either  see the new value or `None`;
            // the old value has been removed!
            assert!(value == None || value == Some(3));
        });

        let idx3 = store_val(&slab, 3);
        t1.join().expect("thread 1 should not panic");

        assert_eq!(get_val(&slab, idx3), Some(3), "slab: {:#?}", slab);
        assert_eq!(get_val(&slab, idx2), Some(2), "slab: {:#?}", slab);
    });
}

#[test]
fn concurrent_remove_remote_and_reuse() {
    let mut model = loom::model::Builder::new();
    model.max_branches = 100000;
    model.preemption_bound = 2; // otherwise this will run *forever*...
    let iter = std::sync::atomic::AtomicUsize::new(1);
    model.check(move || {
        println!(
            "\n --- iteration {} ---\n",
            iter.fetch_add(1, Ordering::Relaxed)
        );

        let slab = Arc::new(Slab::new());

        let idx1 = store_val(&slab, 1);
        let idx2 = store_val(&slab, 2);

        assert_eq!(get_val(&slab, idx1), Some(1), "slab: {:#?}", slab);
        assert_eq!(get_val(&slab, idx2), Some(2), "slab: {:#?}", slab);

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

        assert!(get_val(&slab, idx1).is_none(), "slab: {:#?}", slab);
        assert!(get_val(&slab, idx2).is_none(), "slab: {:#?}", slab);
        assert_eq!(get_val(&slab, idx3), Some(3), "slab: {:#?}", slab);
    });
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
//             assert_eq!(get_val(&slab, k), Some(i), "slab: {:#?}", slab);
//         }
//     });
// }
