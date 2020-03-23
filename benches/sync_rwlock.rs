use bencher::{black_box, Bencher};
use std::sync::Arc;
use tokio::{sync::RwLock, task};

fn read_uncontended(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .core_threads(6)
        .threaded_scheduler()
        .build()
        .unwrap();

    let lock = Arc::new(RwLock::new(()));
    b.iter(|| {
        let lock = lock.clone();
        rt.block_on(async move {
            for _ in 0..6 {
                let read = lock.read().await;
                black_box(read);
            }
        })
    });
}

fn read_concurrent_uncontended_multi(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .core_threads(6)
        .threaded_scheduler()
        .build()
        .unwrap();

    async fn task(lock: Arc<RwLock<()>>) {
        let read = lock.read().await;
        black_box(read);
    }

    let lock = Arc::new(RwLock::new(()));
    b.iter(|| {
        let lock = lock.clone();
        rt.block_on(async move {
            let j = tokio::try_join! {
                task::spawn(task(lock.clone())),
                task::spawn(task(lock.clone())),
                task::spawn(task(lock.clone())),
                task::spawn(task(lock.clone())),
                task::spawn(task(lock.clone())),
                task::spawn(task(lock.clone()))
            };
            j.unwrap();
        })
    });
}

fn read_concurrent_uncontended(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .basic_scheduler()
        .build()
        .unwrap();

    async fn task(lock: Arc<RwLock<()>>) {
        let read = lock.read().await;
        black_box(read);
    }

    let lock = Arc::new(RwLock::new(()));
    b.iter(|| {
        let lock = lock.clone();
        rt.block_on(async move {
            tokio::join! {
                task(lock.clone()),
                task(lock.clone()),
                task(lock.clone()),
                task(lock.clone()),
                task(lock.clone()),
                task(lock.clone())
            };
        })
    });
}

fn read_concurrent_contended_multi(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .core_threads(6)
        .threaded_scheduler()
        .build()
        .unwrap();

    async fn task(lock: Arc<RwLock<()>>) {
        let read = lock.read().await;
        black_box(read);
    }

    let lock = Arc::new(RwLock::new(()));
    b.iter(|| {
        let lock = lock.clone();
        rt.block_on(async move {
            let write = lock.write().await;
            let j = tokio::try_join! {
                async move { drop(write); Ok(()) },
                task::spawn(task(lock.clone())),
                task::spawn(task(lock.clone())),
                task::spawn(task(lock.clone())),
                task::spawn(task(lock.clone())),
                task::spawn(task(lock.clone())),
            };
            j.unwrap();
        })
    });
}

fn read_concurrent_contended(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .basic_scheduler()
        .build()
        .unwrap();

    async fn task(lock: Arc<RwLock<()>>) {
        let read = lock.read().await;
        black_box(read);
    }

    let lock = Arc::new(RwLock::new(()));
    b.iter(|| {
        let lock = lock.clone();
        rt.block_on(async move {
            let write = lock.write().await;
            tokio::join! {
                async move { drop(write) },
                task(lock.clone()),
                task(lock.clone()),
                task(lock.clone()),
                task(lock.clone()),
                task(lock.clone()),
            };
        })
    });
}

bencher::benchmark_group!(
    sync_rwlock,
    read_uncontended,
    read_concurrent_uncontended,
    read_concurrent_uncontended_multi,
    read_concurrent_contended,
    read_concurrent_contended_multi
);

bencher::benchmark_main!(sync_rwlock);
