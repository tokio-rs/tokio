#![warn(rust_2018_idioms)]

use tokio::{net::TcpListener, sync::oneshot};
use tokio_util::context::TokioContext;

use lazy_static::lazy_static;
use std::future::Future;

struct ThreadPool {
    inner: futures::executor::ThreadPool,
    rt: tokio::runtime::Runtime,
}

lazy_static! {
    static ref EXECUTOR: ThreadPool = {
        // Spawn tokio runtime on a single background thread
        // enabling IO and timers.
        let rt = tokio::runtime::Builder::new()
            .basic_scheduler()
            .enable_all()
            .core_threads(1)
            .build()
            .unwrap();

        let inner = futures::executor::ThreadPool::builder().create().unwrap();

        ThreadPool { inner, rt }
    };
}

impl ThreadPool {
    fn spawn(&self, f: impl Future<Output = ()> + Send + 'static) {
        let handle = self.rt.handle().clone();
        self.inner.spawn_ok(TokioContext::new(f, handle));
    }
}

#[test]
fn tokio_context_with_another_runtime() {
    let (tx, rx) = oneshot::channel();

    EXECUTOR.spawn(async move {
        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        println!("addr: {:?}", listener.local_addr());
        tx.send(()).unwrap();
    });

    futures::executor::block_on(rx).unwrap();
}
