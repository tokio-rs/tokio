use futures;
use tokio_threadpool;

use self::tokio_threadpool::Builder;
use futures::sync::oneshot;
use futures::Future;
use std::io;

pub fn run<F>(f: F)
where
    F: Future<Item = (), Error = io::Error> + Send + 'static,
{
    let pool = Builder::new().pool_size(1).build();
    let (tx, rx) = oneshot::channel::<()>();
    pool.spawn(f.then(|_| tx.send(())));
    rx.wait().unwrap()
}
