#![warn(rust_2018_idioms)]

use tokio_executor::threadpool::*;
use tokio_sync::{mpsc, oneshot};

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::*;
use std::sync::Arc;
use std::task::{Context, Poll};

#[test]
fn hammer() {
    const N: usize = 1000;
    const ITER: usize = 20;

    struct Counted<T> {
        cnt: Arc<AtomicUsize>,
        inner: T,
    }

    impl<T: Future> Future for Counted<T> {
        type Output = T::Output;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T::Output> {
            unsafe {
                let inner = &mut self.get_unchecked_mut().inner;
                Pin::new_unchecked(inner).poll(cx)
            }
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

        let (mut listen_tx, mut listen_rx) =
            mpsc::unbounded_channel::<oneshot::Sender<oneshot::Sender<()>>>();

        pool.spawn({
            let c1 = cnt.clone();
            let c2 = cnt.clone();
            let pool = pool.sender().clone();
            let task = async move {
                while let Some(tx) = listen_rx.recv().await {
                    let task = async {
                        let (tx2, rx2) = oneshot::channel();
                        tx.send(tx2).unwrap();
                        rx2.await.unwrap()
                    };

                    pool.spawn(Counted {
                        inner: task,
                        cnt: c1.clone(),
                    })
                    .unwrap();
                }
            };

            /*
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
                */

            Counted {
                inner: task,
                cnt: c2,
            }
        });

        for _ in 0..N {
            let cnt = cnt.clone();
            let (tx, rx) = oneshot::channel();
            listen_tx.try_send(tx).unwrap();

            pool.spawn(async {
                let task = async {
                    let tx = rx.await.unwrap();
                    tx.send(()).unwrap();
                };

                /*
                let task = rx.map_err(|e| panic!("rx err={:?}", e)).and_then(|tx| {
                    tx.send(()).unwrap();
                    Ok(())
                });
                */

                Counted { inner: task, cnt }.await
            });
        }

        drop(listen_tx);

        pool.shutdown_on_idle().wait();
        assert_eq!(N * 2 + 1, cnt.load(Relaxed));
    }
}
