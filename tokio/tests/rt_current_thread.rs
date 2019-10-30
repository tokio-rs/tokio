#![warn(rust_2018_idioms)]

use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tokio::timer;
use tokio_test::{assert_err, assert_ok};

use futures_util::future::poll_fn;
use std::sync::{mpsc, Arc};
use std::task::Poll;
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
fn spawn_one() {
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

    const ITER: usize = 10;

    let mut rt = rt();

    let out = rt.block_on(async {
        let (done_tx, mut done_rx) = mpsc::unbounded_channel();

        let mut txs = (0..ITER)
            .map(|i| {
                let (tx, rx) = oneshot::channel();
                let mut done_tx = done_tx.clone();

                tokio::spawn(async move {
                    let msg = assert_ok!(rx.await);
                    assert_eq!(i, msg);
                    assert_ok!(done_tx.try_send(msg));
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
            Poll::Pending
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
    let sp = rt.spawner();

    let (tx, rx) = oneshot::channel();

    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));

        sp.spawn(async move {
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
        timer::delay_for(dur).await;
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
            timer::delay_for(dur).await;
            assert_ok!(tx.send(()));
        });

        assert_ok!(rx.await);
    });

    assert!(now.elapsed() >= dur);
}

#[test]
fn client_server_block_on() {
    let _ = env_logger::try_init();

    let mut rt = rt();
    let (tx, rx) = mpsc::channel();

    rt.block_on(async move { client_server(tx).await });

    assert_ok!(rx.try_recv());
    assert_err!(rx.try_recv());
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

fn rt() -> Runtime {
    tokio::runtime::Builder::new()
        .current_thread()
        .build()
        .unwrap()
}
