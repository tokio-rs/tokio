use crate::runtime::{self, Runtime};

use std::sync::Arc;

#[test]
fn blocking_shutdown() {
    loom::model(|| {
        let v = Arc::new(());

        let rt = mk_runtime(1);
        rt.enter(|| {
            for _ in 0..2 {
                let v = v.clone();
                crate::task::spawn_blocking(move || {
                    assert!(1 < Arc::strong_count(&v));
                });
            }
        });

        drop(rt);
        assert_eq!(1, Arc::strong_count(&v));
    });
}

fn mk_runtime(num_threads: usize) -> Runtime {
    runtime::Builder::new()
        .threaded_scheduler()
        .core_threads(num_threads)
        .build()
        .unwrap()
}
