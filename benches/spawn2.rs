use std::time::Instant;

use criterion::*;
use criterion::black_box;

// fn foo() {
//     // ...
// }
//
// fn spawn_tasks(c: &mut Criterion) {
//     c.bench_function("spawn_tasks", move |b| {
//         b.iter_custom(|iters| {
//             let start = Instant::now();
//             for _i in 0..iters {
//                 black_box(foo());
//             }
//             start.elapsed()
//         })
//     });
// }

fn spawn_tasks_current_thread(c: &mut Criterion) {
    // 先使用默认的 worker threads 配置
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    c.bench_function("spawn_tasks_current_thread", move |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(
                async {
                    black_box(
                        job(iters as usize, 1).await
                    );
                }
            );
            start.elapsed()
        }
        )
    });
}

fn spawn_tasks_current_thread_parallel(c: &mut Criterion) {
    // 先使用默认的 worker threads 配置
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    c.bench_function("spawn_tasks_current_thread_parallel", move |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(
                async {
                    black_box(
                        job(iters as usize, num_cpus::get_physical()).await
                    );
                }
            );
            start.elapsed()
        }
        )
    });
}


fn spawn_tasks(c: &mut Criterion) {
    // 先使用默认的 worker threads 配置
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .build()
        .unwrap();

    c.bench_function("spawn_tasks", move |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(
                async {
                    black_box(
                        job(iters as usize, 1).await
                    );
                }
            );
            start.elapsed()
        }
        )
    });
}

fn spawn_tasks_parallel(c: &mut Criterion) {
    // 先使用默认的 worker threads 配置
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .build()
        .unwrap();
    c.bench_function("spawn_tasks_parallel", move |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(
                async {
                    black_box(
                        job(iters as usize, num_cpus::get_physical()).await
                    );
                }
            );
            start.elapsed()
        }
        )
    });
}

async fn job(iters: usize, procs: usize) {
    // cpu number 个线程并发负责 spwan task
    for _ in 0..procs {
        let mut threads_handles = Vec::with_capacity(procs);
        // 每一个线程负责的任务是，进行 iters/num_cpus 个 task 的启动
        threads_handles.push(tokio::spawn(async move {
            let mut thread_handles = Vec::with_capacity(iters / procs);
            for _ in 0..iters / procs {
                thread_handles.push(tokio::spawn(
                    async {
                        let val = 1 + 1;
                        tokio::task::yield_now().await;
                        black_box(val)
                    }
                ));
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

criterion_group!(
    benches,
    spawn_tasks_current_thread,
    spawn_tasks_current_thread_parallel,
    spawn_tasks,
    spawn_tasks_parallel
);
criterion_main!(benches);