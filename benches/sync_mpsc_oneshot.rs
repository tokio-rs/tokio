use bencher::{benchmark_group, benchmark_main, Bencher};
use tokio::{
    runtime::Runtime,
    sync::{mpsc, oneshot},
};

fn request_reply_current_thread(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    request_reply(b, rt);
}

fn request_reply_multi_threaded(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .unwrap();

    request_reply(b, rt);
}

fn request_reply(b: &mut Bencher, rt: Runtime) {
    let tx = rt.block_on(async move {
        let (tx, mut rx) = mpsc::channel::<oneshot::Sender<()>>(10);
        tokio::spawn(async move {
            while let Some(reply) = rx.recv().await {
                reply.send(()).unwrap();
            }
        });
        tx
    });

    b.iter(|| {
        let task_tx = tx.clone();
        rt.block_on(async move {
            for _ in 0..1_000 {
                let (o_tx, o_rx) = oneshot::channel();
                task_tx.send(o_tx).await.unwrap();
                let _ = o_rx.await;
            }
        })
    });
}

benchmark_group!(
    sync_mpsc_oneshot_group,
    request_reply_current_thread,
    request_reply_multi_threaded,
);

benchmark_main!(sync_mpsc_oneshot_group);
