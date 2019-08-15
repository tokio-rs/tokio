use tokio_executor::threadpool::Builder;

use std::future::Future;
use std::io;
use std::sync::mpsc;

pub fn run<F>(f: F)
where
    F: Future<Output = io::Result<()>> + Send + 'static,
{
    let pool = Builder::new().pool_size(1).build();
    let (tx, rx) = mpsc::channel();
    pool.spawn(async move {
        f.await.unwrap();
        tx.send(()).unwrap();
    });
    rx.recv().unwrap()
}
