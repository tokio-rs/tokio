use tokio_threadpool;

use self::tokio_threadpool::Builder;
use futures_channel::oneshot;
use futures_util::future::FutureExt;
use std::future::Future;
use std::io;

pub fn run<F: Future>(f: F)
where
    F: Future<Output = io::Result<()>> + Send + 'static,
{
    let pool = Builder::new().pool_size(1).build();
    let (tx, rx) = oneshot::channel::<()>();
    pool.spawn(f.then(|_| tx.send(()).unwrap()));
    tokio::runtime::current_thread::spawn(rx).unwrap()
}
