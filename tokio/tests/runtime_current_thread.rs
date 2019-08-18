#![warn(rust_2018_idioms)]
#![feature(async_await)]
#![cfg(feature = "default")]

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::current_thread::Runtime;
use tokio::sync::oneshot;
use tokio_test::{assert_err, assert_ok};

use env_logger;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tokio::timer::delay;

async fn client_server(tx: mpsc::Sender<()>) {
    let addr = assert_ok!("127.0.0.1:0".parse());
    let mut server = assert_ok!(TcpListener::bind(&addr));

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
fn spawn_run_spawn_root() {
    let _ = env_logger::try_init();

    let mut rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel();

    let tx2 = tx.clone();
    rt.spawn(async move {
        delay(Instant::now() + Duration::from_millis(1000)).await;
        tx2.send(()).unwrap();
    });

    rt.spawn(client_server(tx));
    rt.run().unwrap();

    assert_ok!(rx.try_recv());
    assert_ok!(rx.try_recv());
}

#[test]
fn spawn_run_nested_spawn() {
    let _ = env_logger::try_init();

    let mut rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel();

    let tx2 = tx.clone();
    rt.spawn(async move {
        tokio::spawn(async move {
            delay(Instant::now() + Duration::from_millis(1000)).await;
            tx2.send(()).unwrap();
        });
    });

    rt.spawn(client_server(tx));
    rt.run().unwrap();

    assert_ok!(rx.try_recv());
    assert_ok!(rx.try_recv());
}

#[test]
fn block_on() {
    let _ = env_logger::try_init();

    let mut rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel();

    let tx2 = tx.clone();
    rt.spawn(async move {
        delay(Instant::now() + Duration::from_millis(1000)).await;
        tx2.send(()).unwrap();
    });

    rt.block_on(client_server(tx));

    assert_ok!(rx.try_recv());
    assert_err!(rx.try_recv());
}

#[test]
fn racy() {
    use std::sync::mpsc;
    use std::thread;

    let (trigger, exit) = oneshot::channel();
    let (handle_tx, handle_rx) = mpsc::channel();

    let jh = thread::spawn(move || {
        let mut rt = Runtime::new().unwrap();
        handle_tx.send(rt.handle()).unwrap();

        // don't exit until we are told to
        rt.block_on(async {
            exit.await.unwrap();
        });

        // run until all spawned futures (incl. the "exit" signal future) have completed.
        rt.run().unwrap();
    });

    let (tx, rx) = oneshot::channel();

    let handle = handle_rx.recv().unwrap();
    handle
        .spawn(async {
            tx.send(()).unwrap();
        })
        .unwrap();

    // signal runtime thread to exit
    trigger.send(()).unwrap();

    // wait for runtime thread to exit
    jh.join().unwrap();

    let mut e = tokio_executor::enter().unwrap();
    e.block_on(rx).unwrap();
}
