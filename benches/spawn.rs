//! Benchmark spawning a task onto the basic and threaded Tokio executors.
//! This essentially measure the time to enqueue a task in the local and remote
//! case.

use bencher::{black_box, Bencher};

async fn work() -> usize {
    let val = 1 + 1;
    black_box(val)
}

fn basic_scheduler_local_spawn(bench: &mut Bencher) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        bench.iter(|| {
            let h = tokio::spawn(work());
            black_box(h);
        })
    });
}

fn threaded_scheduler_local_spawn(bench: &mut Bencher) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        bench.iter(|| {
            let h = tokio::spawn(work());
            black_box(h);
        })
    });
}

fn basic_scheduler_remote_spawn(bench: &mut Bencher) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    bench.iter(|| {
        let h = runtime.spawn(work());
        black_box(h);
    });
}

fn threaded_scheduler_remote_spawn(bench: &mut Bencher) {
    let runtime = tokio::runtime::Builder::new_multi_thread().build().unwrap();

    bench.iter(|| {
        let h = runtime.spawn(work());
        black_box(h);
    });
}

bencher::benchmark_group!(
    spawn,
    basic_scheduler_local_spawn,
    threaded_scheduler_local_spawn,
    basic_scheduler_remote_spawn,
    threaded_scheduler_remote_spawn
);

bencher::benchmark_main!(spawn);
