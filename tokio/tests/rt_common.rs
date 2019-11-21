// Tests to run on both current-thread & therad-pool runtime variants.

#![warn(rust_2018_idioms)]

macro_rules! rt_test {
    ($($t:tt)*) => {
        mod basic_scheduler {
            $($t)*

            fn rt() -> Runtime {
                tokio::runtime::Builder::new()
                    .basic_scheduler()
                    .enable_all()
                    .build()
                    .unwrap()
            }
        }

        mod threaded_scheduler {
            $($t)*

            fn rt() -> Runtime {
                tokio::runtime::Builder::new()
                    .threaded_scheduler()
                    .enable_all()
                    .build()
                    .unwrap()
            }
        }
    }
}

#[test]
fn send_sync_bound() {
    use tokio::runtime::Runtime;
    fn is_send<T: Send + Sync>() {}

    is_send::<Runtime>();
}

rt_test! {
    use tokio::net::{TcpListener, TcpStream};
    use tokio::prelude::*;
    use tokio::runtime::Runtime;
    use tokio::sync::oneshot;
    use tokio::time;
    use tokio_test::{assert_err, assert_ok};

    use futures::future::poll_fn;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{mpsc, Arc};
    use std::task::{Context, Poll};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn block_on_sync() {
        let mut rt = rt();

        let mut win = false;
        rt.block_on(async {
            win = true;
        });

        assert!(win);
    }

    #[test]
    fn block_on_async() {
        let mut rt = rt();

        let out = rt.block_on(async {
            let (tx, rx) = oneshot::channel();

            thread::spawn(move || {
                thread::sleep(Duration::from_millis(50));
                tx.send("ZOMG").unwrap();
            });

            assert_ok!(rx.await)
        });

        assert_eq!(out, "ZOMG");
    }

    #[test]
    fn spawn_one_bg() {
        let mut rt = rt();

        let out = rt.block_on(async {
            let (tx, rx) = oneshot::channel();

            tokio::spawn(async move {
                tx.send("ZOMG").unwrap();
            });

            assert_ok!(rx.await)
        });

        assert_eq!(out, "ZOMG");
    }

    #[test]
    fn spawn_one_join() {
        let mut rt = rt();

        let out = rt.block_on(async {
            let (tx, rx) = oneshot::channel();

            let handle = tokio::spawn(async move {
                tx.send("ZOMG").unwrap();
                "DONE"
            });

            let msg = assert_ok!(rx.await);

            let out = assert_ok!(handle.await);
            assert_eq!(out, "DONE");

            msg
        });

        assert_eq!(out, "ZOMG");
    }

    #[test]
    fn spawn_two() {
        let mut rt = rt();

        let out = rt.block_on(async {
            let (tx1, rx1) = oneshot::channel();
            let (tx2, rx2) = oneshot::channel();

            tokio::spawn(async move {
                assert_ok!(tx1.send("ZOMG"));
            });

            tokio::spawn(async move {
                let msg = assert_ok!(rx1.await);
                assert_ok!(tx2.send(msg));
            });

            assert_ok!(rx2.await)
        });

        assert_eq!(out, "ZOMG");
    }

    #[test]
    fn spawn_many() {
        use tokio::sync::mpsc;

        const ITER: usize = 20;

        let mut rt = rt();

        let out = rt.block_on(async {
            let (done_tx, mut done_rx) = mpsc::unbounded_channel();

            let mut txs = (0..ITER)
                .map(|i| {
                    let (tx, rx) = oneshot::channel();
                    let done_tx = done_tx.clone();

                    tokio::spawn(async move {
                        let msg = assert_ok!(rx.await);
                        assert_eq!(i, msg);
                        assert_ok!(done_tx.send(msg));
                    });

                    tx
                })
                .collect::<Vec<_>>();

            drop(done_tx);

            thread::spawn(move || {
                for (i, tx) in txs.drain(..).enumerate() {
                    assert_ok!(tx.send(i));
                }
            });

            let mut out = vec![];
            while let Some(i) = done_rx.recv().await {
                out.push(i);
            }

            out.sort();
            out
        });

        assert_eq!(ITER, out.len());

        for i in 0..ITER {
            assert_eq!(i, out[i]);
        }
    }

    #[test]
    fn outstanding_tasks_dropped() {
        let mut rt = rt();

        let cnt = Arc::new(());

        rt.block_on(async {
            let cnt = cnt.clone();

            tokio::spawn(poll_fn(move |_| {
                assert_eq!(2, Arc::strong_count(&cnt));
                Poll::<()>::Pending
            }));
        });

        assert_eq!(2, Arc::strong_count(&cnt));

        drop(rt);

        assert_eq!(1, Arc::strong_count(&cnt));
    }

    #[test]
    #[should_panic]
    fn nested_rt() {
        let mut rt1 = rt();
        let mut rt2 = rt();

        rt1.block_on(async { rt2.block_on(async { "hello" }) });
    }

    #[test]
    fn create_rt_in_block_on() {
        let mut rt1 = rt();
        let mut rt2 = rt1.block_on(async { rt() });
        let out = rt2.block_on(async { "ZOMG" });

        assert_eq!(out, "ZOMG");
    }

    #[test]
    fn complete_block_on_under_load() {
        let mut rt = rt();

        rt.block_on(async {
            let (tx, rx) = oneshot::channel();

            // Spin hard
            tokio::spawn(async {
                loop {
                    yield_once().await;
                }
            });

            thread::spawn(move || {
                thread::sleep(Duration::from_millis(50));
                assert_ok!(tx.send(()));
            });

            assert_ok!(rx.await);
        });
    }

    #[test]
    fn complete_task_under_load() {
        let mut rt = rt();

        rt.block_on(async {
            let (tx1, rx1) = oneshot::channel();
            let (tx2, rx2) = oneshot::channel();

            // Spin hard
            tokio::spawn(async {
                loop {
                    yield_once().await;
                }
            });

            thread::spawn(move || {
                thread::sleep(Duration::from_millis(50));
                assert_ok!(tx1.send(()));
            });

            tokio::spawn(async move {
                assert_ok!(rx1.await);
                assert_ok!(tx2.send(()));
            });

            assert_ok!(rx2.await);
        });
    }

    #[test]
    fn spawn_from_other_thread() {
        let mut rt = rt();
        let handle = rt.handle().clone();

        let (tx, rx) = oneshot::channel();

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));

            handle.spawn(async move {
                assert_ok!(tx.send(()));
            });
        });

        rt.block_on(async move {
            assert_ok!(rx.await);
        });
    }

    #[test]
    fn delay_at_root() {
        let mut rt = rt();

        let now = Instant::now();
        let dur = Duration::from_millis(50);

        rt.block_on(async move {
            time::delay_for(dur).await;
        });

        assert!(now.elapsed() >= dur);
    }

    #[test]
    fn delay_in_spawn() {
        let mut rt = rt();

        let now = Instant::now();
        let dur = Duration::from_millis(50);

        rt.block_on(async move {
            let (tx, rx) = oneshot::channel();

            tokio::spawn(async move {
                time::delay_for(dur).await;
                assert_ok!(tx.send(()));
            });

            assert_ok!(rx.await);
        });

        assert!(now.elapsed() >= dur);
    }

    #[test]
    fn block_on_socket() {
        let mut rt = rt();

        rt.block_on(async move {
            let (tx, rx) = oneshot::channel();

            let mut listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();

            tokio::spawn(async move {
                let _ = listener.accept().await;
                tx.send(()).unwrap();
            });

            TcpStream::connect(&addr).await.unwrap();
            rx.await.unwrap();
        });
    }

    #[test]
    fn spawn_from_blocking() {
        let mut rt = rt();

        let out = rt.block_on(async move {
            let inner = assert_ok!(tokio::task::spawn_blocking(|| {
                tokio::spawn(async move { "hello" })
            }).await);

            assert_ok!(inner.await)
        });

        assert_eq!(out, "hello")
    }

    #[test]
    fn delay_from_blocking() {
        let mut rt = rt();

        rt.block_on(async move {
            assert_ok!(tokio::task::spawn_blocking(|| {
                let now = std::time::Instant::now();
                let dur = Duration::from_millis(1);

                // use the futures' block_on fn to make sure we aren't setting
                // any Tokio context
                futures::executor::block_on(async {
                    tokio::time::delay_for(dur).await;
                });

                assert!(now.elapsed() >= dur);
            }).await);
        });
    }

    #[test]
    fn socket_from_blocking() {
        let mut rt = rt();

        rt.block_on(async move {
            let mut listener = assert_ok!(TcpListener::bind("127.0.0.1:0").await);
            let addr = assert_ok!(listener.local_addr());

            let peer = tokio::task::spawn_blocking(move || {
                // use the futures' block_on fn to make sure we aren't setting
                // any Tokio context
                futures::executor::block_on(async {
                    assert_ok!(TcpStream::connect(addr).await);
                });
            });

            // Wait for the client to connect
            let _ = assert_ok!(listener.accept().await);

            assert_ok!(peer.await);
        });
    }

    #[test]
    fn io_driver_called_when_under_load() {
        let mut rt = rt();

        // Create a lot of constant load. The scheduler will always be busy.
        for _ in 0..100 {
            rt.spawn(async {
                loop {
                    tokio::task::yield_now().await;
                }
            });
        }

        // Do some I/O work
        rt.block_on(async {
            let mut listener = assert_ok!(TcpListener::bind("127.0.0.1:0").await);
            let addr = assert_ok!(listener.local_addr());

            let srv = tokio::spawn(async move {
                let (mut stream, _) = assert_ok!(listener.accept().await);
                assert_ok!(stream.write_all(b"hello world").await);
            });

            let cli = tokio::spawn(async move {
                let mut stream = assert_ok!(TcpStream::connect(addr).await);
                let mut dst = vec![0; 11];

                assert_ok!(stream.read_exact(&mut dst).await);
                assert_eq!(dst, b"hello world");
            });

            assert_ok!(srv.await);
            assert_ok!(cli.await);
        });
    }

    #[test]
    fn client_server_block_on() {
        let mut rt = rt();
        let (tx, rx) = mpsc::channel();

        rt.block_on(async move { client_server(tx).await });

        assert_ok!(rx.try_recv());
        assert_err!(rx.try_recv());
    }

    #[test]
    fn panic_in_task() {
        let mut rt = rt();
        let (tx, rx) = oneshot::channel();

        struct Boom(Option<oneshot::Sender<()>>);

        impl Future for Boom {
            type Output = ();

            fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
                panic!();
            }
        }

        impl Drop for Boom {
            fn drop(&mut self) {
                assert!(::std::thread::panicking());
                self.0.take().unwrap().send(()).unwrap();
            }
        }

        rt.spawn(Boom(Some(tx)));
        assert_ok!(rt.block_on(rx));
    }

    #[test]
    #[should_panic]
    fn panic_in_block_on() {
        let mut rt = rt();
        rt.block_on(async { panic!() });
    }

    async fn yield_once() {
        let mut yielded = false;
        poll_fn(|cx| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        })
        .await
    }

    #[test]
    fn enter_and_spawn() {
        let mut rt = rt();
        let handle = rt.enter(|| {
            tokio::spawn(async {})
        });

        assert_ok!(rt.block_on(handle));
    }

    #[test]
    fn eagerly_drops_futures_on_shutdown() {
        use std::sync::mpsc;

        struct Never {
            drop_tx: mpsc::Sender<()>,
        }

        impl Future for Never {
            type Output = ();

            fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
                Poll::Pending
            }
        }

        impl Drop for Never {
            fn drop(&mut self) {
                self.drop_tx.send(()).unwrap();
            }
        }

        let mut rt = rt();

        let (drop_tx, drop_rx) = mpsc::channel();
        let (run_tx, run_rx) = oneshot::channel();

        rt.block_on(async move {
            tokio::spawn(async move {
                assert_ok!(run_tx.send(()));

                Never { drop_tx }.await
            });

            assert_ok!(run_rx.await);
        });

        drop(rt);

        assert_ok!(drop_rx.recv());
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
}
