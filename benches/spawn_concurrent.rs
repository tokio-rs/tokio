use std::time::Instant;

use criterion::*;

fn spawn_tasks_current_thread(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    c.bench_function("spawn_tasks_current_thread", move |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(async {
                black_box(job(iters as usize, 1).await);
            });
            start.elapsed()
        })
    });
}

fn spawn_tasks_current_thread_parallel(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    c.bench_function("spawn_tasks_current_thread_parallel", move |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(async {
                black_box(job(iters as usize, num_cpus::get_physical() * 2).await);
            });
            start.elapsed()
        })
    });
}

fn spawn_tasks(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread().build().unwrap();

    c.bench_function("spawn_tasks", move |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(async {
                black_box(job(iters as usize, 1).await);
            });
            start.elapsed()
        })
    });
}

fn spawn_tasks_parallel<const S: usize>(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .spawn_concurrency_level(S)
        .build()
        .unwrap();
    c.bench_function(format!("spawn_tasks_parallel {}", S).as_str(), move |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(async {
                black_box(job(iters as usize, num_cpus::get_physical()).await);
            });
            start.elapsed()
        })
    });
}

async fn job(iters: usize, procs: usize) {
    for _ in 0..procs {
        let mut threads_handles = Vec::with_capacity(procs);
        threads_handles.push(tokio::spawn(async move {
            let mut thread_handles = Vec::with_capacity(iters / procs);
            for _ in 0..iters / procs {
                thread_handles.push(tokio::spawn(async {
                    let val = 1 + 1;
                    tokio::task::yield_now().await;
                    black_box(val)
                }));
            }
            for handle in thread_handles {
                handle.await.unwrap();
            }
        }));
        for handle in threads_handles {
            handle.await.unwrap();
        }
    }
}

fn shutdown_tasks_parallel<const S: usize>(c: &mut Criterion) {
    c.bench_function(
        format!("showdown_tasks_parallel {}", S).as_str(),
        move |b| {
            b.iter_custom(|iters| {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .spawn_concurrency_level(S)
                    .enable_time()
                    .build()
                    .unwrap();
                runtime.block_on(async {
                    black_box(job_shutdown(iters as usize, num_cpus::get_physical()).await);
                });
                let start = Instant::now();
                drop(runtime);
                start.elapsed()
            })
        },
    );
}

async fn job_shutdown(iters: usize, procs: usize) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    for _ in 0..procs {
        let tx = tx.clone();
        tokio::spawn(async move {
            for _ in 0..iters / procs {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let val = 1 + 1;
                    tx.send(()).unwrap();
                    tokio::time::sleep(tokio::time::Duration::from_secs(1000)).await; // it will never return
                    black_box(val)
                });
            }
        });
    }
    for _ in 0..(iters / procs) * procs {
        rx.recv().await;
    }
}

criterion_group!(
    benches,
    spawn_tasks_current_thread,
    spawn_tasks_current_thread_parallel,
    spawn_tasks,
    spawn_tasks_parallel<1>,
    spawn_tasks_parallel<4>,
    spawn_tasks_parallel<8>,
    spawn_tasks_parallel<16>,
    spawn_tasks_parallel<32>,
    spawn_tasks_parallel<64>,
    spawn_tasks_parallel<128>,
    spawn_tasks_parallel<256>,
    spawn_tasks_parallel<512>,
    spawn_tasks_parallel<1024>,
    shutdown_tasks_parallel<1>,
    shutdown_tasks_parallel<4>,
    shutdown_tasks_parallel<8>,
    shutdown_tasks_parallel<16>,
    shutdown_tasks_parallel<32>,
    shutdown_tasks_parallel<64>,
    shutdown_tasks_parallel<128>,
    shutdown_tasks_parallel<256>,
    shutdown_tasks_parallel<512>,
    shutdown_tasks_parallel<1024>,
);
criterion_main!(benches);
