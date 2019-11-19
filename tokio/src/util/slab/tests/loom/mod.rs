use self::test_util::*;
use super::super::Slab;
use loom::sync::{Arc, Condvar, Mutex};
use loom::thread;

pub(crate) mod test_util {
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub(crate) fn run_model(name: &'static str, f: impl Fn() + Sync + Send + 'static) {
        run_builder(name, loom::model::Builder::new(), f)
    }

    pub(crate) fn run_builder(
        name: &'static str,
        builder: loom::model::Builder,
        f: impl Fn() + Sync + Send + 'static,
    ) {
        let iters = AtomicUsize::new(1);
        builder.check(move || {
            println!(
                "\n------------ running test {}; iteration {} ------------\n",
                name,
                iters.fetch_add(1, Ordering::SeqCst)
            );
            f()
        });
    }
}

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
    slab.get(key).and_then(|s| s.get_readiness(key))
}

mod single_shard;
// mod small_slab;


// #[test]
// fn unique_iter() {
//     run_model("unique_iter", || {
//         let mut slab = Arc::new(Slab::new());

//         let s = slab.clone();
//         let t1 = thread::spawn(move || {
//             store_val(&s, 1);
//             store_val(&s, 2);
//         });

//         let s = slab.clone();
//         let t2 = thread::spawn(move || {
//             store_val(&s, 3);
//             store_val(&s, 4);
//         });

//         t1.join().expect("thread 1 should not panic");
//         t2.join().expect("thread 2 should not panic");

//         let slab = Arc::get_mut(&mut slab).expect("other arcs should be dropped");
//         let items: Vec<_> = slab
//             .unique_iter()
//             .map(|i| i.readiness.load(Ordering::Acquire))
//             .collect();
//         assert!(items.contains(&1), "items: {:?}", items);
//         assert!(items.contains(&2), "items: {:?}", items);
//         assert!(items.contains(&3), "items: {:?}", items);
//         assert!(items.contains(&4), "items: {:?}", items);
//     });
// }
