//! Benchmark spawning a task onto the basic and threaded Tokio executors.
//! This essentially measure the time to enqueue a task in the local and remote
//! case.

#[macro_use]
extern crate bencher;

use bencher::{black_box, Bencher};

fn time_now_current_thread(bench: &mut Bencher) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    bench.iter(|| {
        rt.block_on(async {
            black_box(tokio::time::Instant::now());
        })
    })
}

bencher::benchmark_group!(time_now, time_now_current_thread,);

bencher::benchmark_main!(time_now);
