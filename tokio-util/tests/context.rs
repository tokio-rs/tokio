#![warn(rust_2018_idioms)]

use tokio::{net::TcpListener, sync::oneshot};
use tokio_util::context::{HandleExt, TokioContext};

use std::future::Future;


use tokio::runtime::Builder;

struct ThreadPool {
    inner: futures::executor::ThreadPool,
    rt: tokio::runtime::Runtime,
}

impl ThreadPool {
    fn spawn(&self, f: impl Future<Output = ()> + Send + 'static) {
        let handle = self.rt.handle().clone();
        let h: TokioContext<_> = handle.wrap(f);
        self.inner.spawn_ok(h);
    }
}

#[test]
fn tokio_context_with_another_runtime() {

    let (tx, rx) = oneshot::channel();
    let custom_executor: ThreadPool = {
        // Spawn tokio runtime on a single background thread
        // enabling IO and timers.
        let rt = Builder::new()
            .basic_scheduler()
            .enable_all()
            .core_threads(1)
            .build()
            .unwrap();

        let inner = futures::executor::ThreadPool::builder().create().unwrap();

        ThreadPool { inner, rt }
    };

    custom_executor.spawn(async move {
        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        println!("addr: {:?}", listener.local_addr());
        tx.send(()).unwrap();
    });

    futures::executor::block_on(rx).unwrap();
}

#[test]
#[should_panic]
fn tokio_context_with_another_runtime_no_timer_io() {
    let (tx, rx) = oneshot::channel();
    let custom_executor: ThreadPool = {
        // Spawn tokio runtime on a single background thread
        // enabling IO and timers.
        let rt = Builder::new()
            .basic_scheduler()
            .core_threads(1)
            .build()
            .unwrap();

        let inner = futures::executor::ThreadPool::builder().create().unwrap();

        ThreadPool { inner, rt }
    };

    custom_executor.spawn(async move {
        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        println!("addr: {:?}", listener.local_addr());
        tx.send(()).unwrap();
    });

    // panics: "there is no reactor running, must be called from the context 
    // of Tokio runtime"
    futures::executor::block_on(rx).unwrap();
}
