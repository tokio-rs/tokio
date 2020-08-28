use bencher::Bencher;
use std::sync::Arc;
use tokio::{sync::Semaphore, task};

fn uncontended(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new()
        .core_threads(6)
        .threaded_scheduler()
        .build()
        .unwrap();

    let s = Arc::new(Semaphore::new(10));
    b.iter(|| {
        let s = s.clone();
        rt.block_on(async move {
            for _ in 0..6 {
                let permit = s.acquire().await;
                drop(permit);
            }
        })
    });
}

async fn task(s: Arc<Semaphore>) {
    let permit = s.acquire().await;
    drop(permit);
}

fn uncontended_concurrent_multi(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new()
        .core_threads(6)
        .threaded_scheduler()
        .build()
        .unwrap();

    let s = Arc::new(Semaphore::new(10));
    b.iter(|| {
        let s = s.clone();
        rt.block_on(async move {
            let j = tokio::try_join! {
                task::spawn(task(s.clone())),
                task::spawn(task(s.clone())),
                task::spawn(task(s.clone())),
                task::spawn(task(s.clone())),
                task::spawn(task(s.clone())),
                task::spawn(task(s.clone()))
            };
            j.unwrap();
        })
    });
}

fn uncontended_concurrent_single(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new()
        .basic_scheduler()
        .build()
        .unwrap();

    let s = Arc::new(Semaphore::new(10));
    b.iter(|| {
        let s = s.clone();
        rt.block_on(async move {
            tokio::join! {
                task(s.clone()),
                task(s.clone()),
                task(s.clone()),
                task(s.clone()),
                task(s.clone()),
                task(s.clone())
            };
        })
    });
}

fn contended_concurrent_multi(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new()
        .core_threads(6)
        .threaded_scheduler()
        .build()
        .unwrap();

    let s = Arc::new(Semaphore::new(5));
    b.iter(|| {
        let s = s.clone();
        rt.block_on(async move {
            let j = tokio::try_join! {
                task::spawn(task(s.clone())),
                task::spawn(task(s.clone())),
                task::spawn(task(s.clone())),
                task::spawn(task(s.clone())),
                task::spawn(task(s.clone())),
                task::spawn(task(s.clone()))
            };
            j.unwrap();
        })
    });
}

fn contended_concurrent_single(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new()
        .basic_scheduler()
        .build()
        .unwrap();

    let s = Arc::new(Semaphore::new(5));
    b.iter(|| {
        let s = s.clone();
        rt.block_on(async move {
            tokio::join! {
                task(s.clone()),
                task(s.clone()),
                task(s.clone()),
                task(s.clone()),
                task(s.clone()),
                task(s.clone())
            };
        })
    });
}

bencher::benchmark_group!(
    sync_semaphore,
    uncontended,
    uncontended_concurrent_multi,
    uncontended_concurrent_single,
    contended_concurrent_multi,
    contended_concurrent_single
);

bencher::benchmark_main!(sync_semaphore);
