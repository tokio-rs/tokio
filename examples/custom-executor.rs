// This example shows how to use the tokio runtime with any other executor
//
// The main components are a spawn fn that will wrap futures in a special future
// that will always enter the tokio context on poll. This only spawns one extra thread
// to manage and run the tokio drivers in the background.

use tokio::net::TcpListener;
use tokio::sync::oneshot;

fn main() {
    let (tx, rx) = oneshot::channel();

    my_custom_runtime::spawn(async move {
        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();

        println!("addr: {:?}", listener.local_addr());

        tx.send(()).unwrap();
    });

    futures::executor::block_on(rx).unwrap();
}

mod my_custom_runtime {
    use once_cell::sync::Lazy;
    use std::future::Future;
    use tokio_util::context::TokioContext;

    pub fn spawn(f: impl Future<Output = ()> + Send + 'static) {
        EXECUTOR.spawn(f);
    }

    struct ThreadPool {
        inner: futures::executor::ThreadPool,
        rt: tokio::runtime::Runtime,
    }

    static EXECUTOR: Lazy<ThreadPool> = Lazy::new(|| {
        // Spawn tokio runtime on a single background thread
        // enabling IO and timers.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let inner = futures::executor::ThreadPool::builder().create().unwrap();

        ThreadPool { inner, rt }
    });

    impl ThreadPool {
        fn spawn(&self, f: impl Future<Output = ()> + Send + 'static) {
            let handle = self.rt.handle().clone();
            self.inner.spawn_ok(TokioContext::new(f, handle));
        }
    }
}
