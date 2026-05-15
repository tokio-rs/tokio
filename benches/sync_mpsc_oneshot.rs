use tokio::{
    runtime::Runtime,
    sync::{mpsc, oneshot},
};

use criterion::{criterion_group, criterion_main, Criterion};

fn request_reply_current_thread(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    request_reply(c, rt);
}

fn request_reply_multi_threaded(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .unwrap();

    request_reply(c, rt);
}

fn request_reply(b: &mut Criterion, rt: Runtime) {
    let tx = rt.block_on(async move {
        let (tx, mut rx) = mpsc::channel::<oneshot::Sender<()>>(10);
        tokio::spawn(async move {
            while let Some(reply) = rx.recv().await {
                reply.send(()).unwrap();
            }
        });
        tx
    });

    b.bench_function("request_reply", |b| {
        b.iter(|| {
            let task_tx = tx.clone();
            rt.block_on(async move {
                for _ in 0..1_000 {
                    let (o_tx, o_rx) = oneshot::channel();
                    task_tx.send(o_tx).await.unwrap();
                    let _ = o_rx.await;
                }
            })
        })
    });
}

criterion_group!(
    sync_mpsc_oneshot_group,
    request_reply_current_thread,
    request_reply_multi_threaded,
);

criterion_main!(sync_mpsc_oneshot_group);
