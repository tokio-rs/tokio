//! Benchmark implementation details of the threaded scheduler. These benches are
//! intended to be used as a form of regression testing and not as a general
//! purpose benchmark demonstrating real-world performance.

use tokio::runtime::{self, Runtime};

use bencher::{benchmark_group, benchmark_main, Bencher};

const NUM_SPAWN: usize = 1_000;

fn spawn_many_local(b: &mut Bencher) {
    let rt = rt();
    let mut handles = Vec::with_capacity(NUM_SPAWN);

    b.iter(|| {
        rt.block_on(async {
            for _ in 0..NUM_SPAWN {
                handles.push(tokio::spawn(async move {}));
            }

            for handle in handles.drain(..) {
                handle.await.unwrap();
            }
        });
    });
}

fn spawn_many_remote_idle(b: &mut Bencher) {
    let rt = rt();
    let rt_handle = rt.handle();
    let mut handles = Vec::with_capacity(NUM_SPAWN);

    b.iter(|| {
        for _ in 0..NUM_SPAWN {
            handles.push(rt_handle.spawn(async {}));
        }

        rt.block_on(async {
            for handle in handles.drain(..) {
                handle.await.unwrap();
            }
        });
    });
}

fn spawn_many_remote_busy(b: &mut Bencher) {
    let rt = rt();
    let rt_handle = rt.handle();
    let mut handles = Vec::with_capacity(NUM_SPAWN);

    rt.spawn(async {
        fn iter() {
            tokio::spawn(async { iter() });
        }

        iter()
    });

    b.iter(|| {
        for _ in 0..NUM_SPAWN {
            handles.push(rt_handle.spawn(async {}));
        }

        rt.block_on(async {
            for handle in handles.drain(..) {
                handle.await.unwrap();
            }
        });
    });
}

fn rt() -> Runtime {
    runtime::Builder::new_current_thread().build().unwrap()
}

benchmark_group!(
    scheduler,
    spawn_many_local,
    spawn_many_remote_idle,
    spawn_many_remote_busy
);

benchmark_main!(scheduler);
