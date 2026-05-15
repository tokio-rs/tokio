//! Benchmark spawning a task onto the basic and threaded Tokio executors.
//! This essentially measure the time to enqueue a task in the local and remote
//! case.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn time_now_current_thread(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    c.bench_function("time_now_current_thread", |b| {
        b.iter(|| {
            rt.block_on(async {
                black_box(tokio::time::Instant::now());
            })
        })
    });
}

criterion_group!(time_now, time_now_current_thread);

criterion_main!(time_now);
