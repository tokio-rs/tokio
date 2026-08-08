use std::time::{Duration, Instant};
use std::{future::poll_fn, future::Future, task::Poll};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tokio::{
    runtime::Runtime,
    time::{sleep, timeout},
};

// a very quick async task, but might timeout
async fn quick_job() -> usize {
    1
}

fn build_run_time(workers: usize) -> Runtime {
    if workers == 1 {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    } else {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(workers)
            .build()
            .unwrap()
    }
}

fn single_thread_scheduler_timeout(c: &mut Criterion) {
    do_timeout_test(c, 1, "single_thread_timeout");
}

fn multi_thread_scheduler_timeout(c: &mut Criterion) {
    do_timeout_test(c, 8, "multi_thread_timeout-8");
}

fn do_timeout_test(c: &mut Criterion, workers: usize, name: &str) {
    let runtime = build_run_time(workers);
    c.bench_function(name, |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(async {
                black_box(spawn_timeout_job(iters as usize, workers)).await;
            });
            start.elapsed()
        })
    });
}

async fn spawn_timeout_job(iters: usize, procs: usize) {
    let mut handles = Vec::with_capacity(procs);
    for _ in 0..procs {
        handles.push(tokio::spawn(async move {
            for _ in 0..iters / procs {
                let h = timeout(Duration::from_secs(1), quick_job());
                assert_eq!(black_box(h.await.unwrap()), 1);
            }
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
}

fn single_thread_scheduler_sleep(c: &mut Criterion) {
    do_sleep_test(c, 1, "single_thread_sleep");
}

fn multi_thread_scheduler_sleep(c: &mut Criterion) {
    do_sleep_test(c, 8, "multi_thread_sleep-8");
}

fn single_thread_scheduler_timer_registration(c: &mut Criterion) {
    do_timer_registration_test(c, 1, "single_thread_timer_registration");
}

fn multi_thread_scheduler_timer_registration(c: &mut Criterion) {
    do_timer_registration_test(c, 8, "multi_thread_timer_registration-8");
}

fn do_sleep_test(c: &mut Criterion, workers: usize, name: &str) {
    let runtime = build_run_time(workers);

    c.bench_function(name, |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(async {
                black_box(spawn_sleep_job(iters as usize, workers)).await;
            });
            start.elapsed()
        })
    });
}

async fn spawn_sleep_job(iters: usize, procs: usize) {
    let mut handles = Vec::with_capacity(procs);
    for _ in 0..procs {
        handles.push(tokio::spawn(async move {
            for _ in 0..iters / procs {
                let _h = black_box(sleep(Duration::from_secs(1)));
            }
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
}

fn do_timer_registration_test(c: &mut Criterion, workers: usize, name: &str) {
    let runtime = build_run_time(workers);

    c.bench_function(name, |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            runtime.block_on(async {
                let mut handles = Vec::with_capacity(workers);
                let iters = iters as usize;
                let iters_per_worker = iters / workers;
                let remainder = iters % workers;
                for worker in 0..workers {
                    let worker_iters = iters_per_worker + usize::from(worker < remainder);
                    handles.push(tokio::spawn(register_timers(worker_iters)));
                }
                for handle in handles {
                    handle.await.unwrap();
                }
            });
            start.elapsed()
        })
    });
}

async fn register_timers(iters: usize) {
    for _ in 0..iters {
        let timer = sleep(Duration::from_secs(1));
        tokio::pin!(timer);
        poll_fn(|cx| {
            assert!(timer.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
    }
}

criterion_group!(
    timeout_benchmark,
    single_thread_scheduler_timeout,
    multi_thread_scheduler_timeout,
    single_thread_scheduler_sleep,
    multi_thread_scheduler_sleep,
    single_thread_scheduler_timer_registration,
    multi_thread_scheduler_timer_registration
);

criterion_main!(timeout_benchmark);
