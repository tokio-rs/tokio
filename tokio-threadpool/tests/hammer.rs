extern crate futures;
extern crate tokio_threadpool;

use tokio_threadpool::*;

use futures::{Future, Poll, Sink, Stream};

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::*;
use std::sync::Arc;

#[test]
fn hammer() {
    use futures::future;
    use futures::sync::{mpsc, oneshot};

    const N: usize = 1000;
    const ITER: usize = 20;

    struct Counted<T> {
        cnt: Arc<AtomicUsize>,
        inner: T,
    }

    impl<T: Future> Future for Counted<T> {
        type Item = T::Item;
        type Error = T::Error;

        fn poll(&mut self) -> Poll<T::Item, T::Error> {
            self.inner.poll()
        }
    }

    impl<T> Drop for Counted<T> {
        fn drop(&mut self) {
            self.cnt.fetch_add(1, Relaxed);
        }
    }

    for _ in 0..ITER {
        let pool = Builder::new()
            // .pool_size(30)
            .build();

        let cnt = Arc::new(AtomicUsize::new(0));

        let (listen_tx, listen_rx) = mpsc::unbounded::<oneshot::Sender<oneshot::Sender<()>>>();
        let mut listen_tx = listen_tx.wait();

        pool.spawn({
            let c1 = cnt.clone();
            let c2 = cnt.clone();
            let pool = pool.sender().clone();
            let task = listen_rx
                .map_err(|e| panic!("accept error = {:?}", e))
                .for_each(move |tx| {
                    let task = future::lazy(|| {
                        let (tx2, rx2) = oneshot::channel();

                        tx.send(tx2).unwrap();
                        rx2
                    })
                    .map_err(|e| panic!("e={:?}", e))
                    .and_then(|_| Ok(()));

                    pool.spawn(Counted {
                        inner: task,
                        cnt: c1.clone(),
                    })
                    .unwrap();

                    Ok(())
                });

            Counted {
                inner: task,
                cnt: c2,
            }
        });

        for _ in 0..N {
            let cnt = cnt.clone();
            let (tx, rx) = oneshot::channel();
            listen_tx.send(tx).unwrap();

            pool.spawn({
                let task = rx.map_err(|e| panic!("rx err={:?}", e)).and_then(|tx| {
                    tx.send(()).unwrap();
                    Ok(())
                });

                Counted { inner: task, cnt }
            });
        }

        drop(listen_tx);

        pool.shutdown_on_idle().wait().unwrap();
        assert_eq!(N * 2 + 1, cnt.load(Relaxed));
    }
}
