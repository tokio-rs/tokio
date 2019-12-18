#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::{self, Runtime};
use tokio::sync::oneshot;
use tokio_test::{assert_err, assert_ok};

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{mpsc, Arc};
use std::task::{Context, Poll};

#[test]
fn single_thread() {
    // No panic when starting a runtime w/ a single thread
    let _ = runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .core_threads(1)
        .build();
}

#[test]
fn many_oneshot_futures() {
    // used for notifying the main thread
    const NUM: usize = 1_000;

    for _ in 0..5 {
        let (tx, rx) = mpsc::channel();

        let rt = rt();
        let cnt = Arc::new(AtomicUsize::new(0));

        for _ in 0..NUM {
            let cnt = cnt.clone();
            let tx = tx.clone();

            rt.spawn(async move {
                let num = cnt.fetch_add(1, Relaxed) + 1;

                if num == NUM {
                    tx.send(()).unwrap();
                }
            });
        }

        rx.recv().unwrap();

        // Wait for the pool to shutdown
        drop(rt);
    }
}
#[test]
fn many_multishot_futures() {
    use tokio::sync::mpsc;

    const CHAIN: usize = 200;
    const CYCLES: usize = 5;
    const TRACKS: usize = 50;

    for _ in 0..50 {
        let mut rt = rt();
        let mut start_txs = Vec::with_capacity(TRACKS);
        let mut final_rxs = Vec::with_capacity(TRACKS);

        for _ in 0..TRACKS {
            let (start_tx, mut chain_rx) = mpsc::channel(10);

            for _ in 0..CHAIN {
                let (mut next_tx, next_rx) = mpsc::channel(10);

                // Forward all the messages
                rt.spawn(async move {
                    while let Some(v) = chain_rx.recv().await {
                        next_tx.send(v).await.unwrap();
                    }
                });

                chain_rx = next_rx;
            }

            // This final task cycles if needed
            let (mut final_tx, final_rx) = mpsc::channel(10);
            let mut cycle_tx = start_tx.clone();
            let mut rem = CYCLES;

            rt.spawn(async move {
                for _ in 0..CYCLES {
                    let msg = chain_rx.recv().await.unwrap();

                    rem -= 1;

                    if rem == 0 {
                        final_tx.send(msg).await.unwrap();
                    } else {
                        cycle_tx.send(msg).await.unwrap();
                    }
                }
            });

            start_txs.push(start_tx);
            final_rxs.push(final_rx);
        }

        {
            rt.block_on(async move {
                for mut start_tx in start_txs {
                    start_tx.send("ping").await.unwrap();
                }

                for mut final_rx in final_rxs {
                    final_rx.recv().await.unwrap();
                }
            });
        }
    }
}

#[test]
fn spawn_shutdown() {
    let mut rt = rt();
    let (tx, rx) = mpsc::channel();

    rt.block_on(async {
        tokio::spawn(client_server(tx.clone()));
    });

    // Use spawner
    rt.spawn(client_server(tx));

    assert_ok!(rx.recv());
    assert_ok!(rx.recv());

    drop(rt);
    assert_err!(rx.try_recv());
}

async fn client_server(tx: mpsc::Sender<()>) {
    let mut server = assert_ok!(TcpListener::bind("127.0.0.1:0").await);

    // Get the assigned address
    let addr = assert_ok!(server.local_addr());

    // Spawn the server
    tokio::spawn(async move {
        // Accept a socket
        let (mut socket, _) = server.accept().await.unwrap();

        // Write some data
        socket.write_all(b"hello").await.unwrap();
    });

    let mut client = TcpStream::connect(&addr).await.unwrap();

    let mut buf = vec![];
    client.read_to_end(&mut buf).await.unwrap();

    assert_eq!(buf, b"hello");
    tx.send(()).unwrap();
}

#[test]
fn drop_threadpool_drops_futures() {
    for _ in 0..1_000 {
        let num_inc = Arc::new(AtomicUsize::new(0));
        let num_dec = Arc::new(AtomicUsize::new(0));
        let num_drop = Arc::new(AtomicUsize::new(0));

        struct Never(Arc<AtomicUsize>);

        impl Future for Never {
            type Output = ();

            fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
                Poll::Pending
            }
        }

        impl Drop for Never {
            fn drop(&mut self) {
                self.0.fetch_add(1, Relaxed);
            }
        }

        let a = num_inc.clone();
        let b = num_dec.clone();

        let rt = runtime::Builder::new()
            .threaded_scheduler()
            .enable_all()
            .on_thread_start(move || {
                a.fetch_add(1, Relaxed);
            })
            .on_thread_stop(move || {
                b.fetch_add(1, Relaxed);
            })
            .build()
            .unwrap();

        rt.spawn(Never(num_drop.clone()));

        // Wait for the pool to shutdown
        drop(rt);

        // Assert that only a single thread was spawned.
        let a = num_inc.load(Relaxed);
        assert!(a >= 1);

        // Assert that all threads shutdown
        let b = num_dec.load(Relaxed);
        assert_eq!(a, b);

        // Assert that the future was dropped
        let c = num_drop.load(Relaxed);
        assert_eq!(c, 1);
    }
}

#[test]
fn start_stop_callbacks_called() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let after_start = Arc::new(AtomicUsize::new(0));
    let before_stop = Arc::new(AtomicUsize::new(0));

    let after_inner = after_start.clone();
    let before_inner = before_stop.clone();
    let mut rt = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .on_thread_start(move || {
            after_inner.clone().fetch_add(1, Ordering::Relaxed);
        })
        .on_thread_stop(move || {
            before_inner.clone().fetch_add(1, Ordering::Relaxed);
        })
        .build()
        .unwrap();

    let (tx, rx) = oneshot::channel();

    rt.spawn(async move {
        assert_ok!(tx.send(()));
    });

    assert_ok!(rt.block_on(rx));

    drop(rt);

    assert!(after_start.load(Ordering::Relaxed) > 0);
    assert!(before_stop.load(Ordering::Relaxed) > 0);
}

#[test]
fn blocking() {
    // used for notifying the main thread
    const NUM: usize = 1_000;

    for _ in 0..10 {
        let (tx, rx) = mpsc::channel();

        let rt = rt();
        let cnt = Arc::new(AtomicUsize::new(0));

        // there are four workers in the pool
        // so, if we run 4 blocking tasks, we know that handoff must have happened
        let block = Arc::new(std::sync::Barrier::new(5));
        for _ in 0..4 {
            let block = block.clone();
            rt.spawn(async move {
                tokio::task::block_in_place(move || {
                    block.wait();
                    block.wait();
                })
            });
        }
        block.wait();

        for _ in 0..NUM {
            let cnt = cnt.clone();
            let tx = tx.clone();

            rt.spawn(async move {
                let num = cnt.fetch_add(1, Relaxed) + 1;

                if num == NUM {
                    tx.send(()).unwrap();
                }
            });
        }

        rx.recv().unwrap();

        // Wait for the pool to shutdown
        block.wait();
    }
}

#[test]
fn multi_threadpool() {
    use tokio::sync::oneshot;

    let rt1 = rt();
    let rt2 = rt();

    let (tx, rx) = oneshot::channel();
    let (done_tx, done_rx) = mpsc::channel();

    rt2.spawn(async move {
        rx.await.unwrap();
        done_tx.send(()).unwrap();
    });

    rt1.spawn(async move {
        tx.send(()).unwrap();
    });

    done_rx.recv().unwrap();
}

fn rt() -> Runtime {
    Runtime::new().unwrap()
}
