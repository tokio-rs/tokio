use super::super::ScheduledIo;
use loom::sync::Arc;
use loom::thread;

use pack::{Pack, WIDTH};
use slab::Shard;
use slab::Slab;
use tid::Tid;

// Overridden for tests
const INITIAL_PAGE_SIZE: usize = 2;

// Constants not overridden
#[cfg(target_pointer_width = "64")]
const MAX_THREADS: usize = 4096;
#[cfg(target_pointer_width = "32")]
const MAX_THREADS: usize = 2048;
const MAX_PAGES: usize = WIDTH / 4;

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

#[test]
fn remove_remote_and_reuse() {
    let mut model = loom::model::Builder::new();
    model.max_branches = 100000;
    model.check(|| {
        println!("\n --- iteration ---\n");
        let slab = Arc::new(Slab::new());

        let idx1 = slab.insert(1).expect("insert");
        let idx2 = slab.insert(2).expect("insert");

        assert_eq!(slab.get_guard(idx1), Some(1), "slab: {:#?}", slab);
        assert_eq!(slab.get_guard(idx2), Some(2), "slab: {:#?}", slab);

        let s = slab.clone();
        let t1 = thread::spawn(move || {
            s.remove(idx1);
            let value = s.get_guard(idx1);

            // We may or may not see the new value yet, depending on when
            // this occurs, but we must either  see the new value or `None`;
            // the old value has been removed!
            assert!(value == None || value == Some(3));
        });

        let idx3 = slab.insert(3).expect("insert");
        t1.join().expect("thread 1 should not panic");

        assert_eq!(slab.get_guard(idx3), Some(3), "slab: {:#?}", slab);
        assert_eq!(slab.get_guard(idx2), Some(2), "slab: {:#?}", slab);
    });
}

#[test]
fn custom_page_sz() {
    let mut model = loom::model::Builder::new();
    model.max_branches = 100000;
    model.check(|| {
        let slab = Slab::new();

        for i in 0..1024 {
            println!("{}", i);
            let k = slab.insert(i).expect("insert");
            assert_eq!(
                slab.get_guard(k).expect("get_guard"),
                i,
                "slab: {:#?}",
                slab
            );
        }
    });
}
