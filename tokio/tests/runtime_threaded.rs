#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use tokio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tokio::timer::Delay;
use tokio_test::{assert_err, assert_ok};

use env_logger;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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
fn spawn_shutdown() {
    let _ = env_logger::try_init();

    let rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel();

    rt.spawn(client_server(tx.clone()));

    // Use executor trait
    let f = Box::pin(client_server(tx));
    tokio_executor::Executor::spawn(&mut rt.executor(), f).unwrap();

    let mut e = tokio_executor::enter().unwrap();
    e.block_on(rt.shutdown_on_idle());

    assert_ok!(rx.try_recv());
    assert_ok!(rx.try_recv());
    assert_err!(rx.try_recv());
}

#[test]
fn block_on_timer() {
    let rt = Runtime::new().unwrap();

    let v = rt.block_on(async move {
        let delay = Delay::new(Instant::now() + Duration::from_millis(100));
        delay.await;
        42
    });

    assert_eq!(v, 42);

    let mut e = tokio_executor::enter().unwrap();
    e.block_on(rt.shutdown_on_idle());
}

#[test]
fn block_waits() {
    let (a_tx, a_rx) = oneshot::channel();
    let (b_tx, b_rx) = mpsc::channel();

    thread::spawn(|| {
        use std::time::Duration;

        thread::sleep(Duration::from_millis(1000));
        a_tx.send(()).unwrap();
    });

    let rt = Runtime::new().unwrap();
    rt.block_on(async move {
        a_rx.await.unwrap();
        b_tx.send(()).unwrap();
    });

    assert_ok!(b_rx.try_recv());

    let mut e = tokio_executor::enter().unwrap();
    e.block_on(rt.shutdown_on_idle());
}

#[test]
fn spawn_many() {
    const ITER: usize = 200;

    let cnt = Arc::new(Mutex::new(0));
    let rt = Runtime::new().unwrap();

    let c = cnt.clone();
    rt.block_on(async move {
        for _ in 0..ITER {
            let c = c.clone();
            tokio::spawn(async move {
                let mut x = c.lock().unwrap();
                *x = 1 + *x;
            });
        }
    });

    let mut e = tokio_executor::enter().unwrap();
    e.block_on(rt.shutdown_on_idle());

    assert_eq!(ITER, *cnt.lock().unwrap());
}

#[test]
fn nested_enter() {
    use std::panic;

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        assert_err!(tokio_executor::enter());

        // Since this is testing panics in other threads, printing about panics
        // is noisy and can give the impression that the test is ignoring panics.
        //
        // It *is* ignoring them, but on purpose.
        let prev_hook = panic::take_hook();
        panic::set_hook(Box::new(|info| {
            let s = info.to_string();
            if s.starts_with("panicked at 'nested ")
                || s.starts_with("panicked at 'Multiple executors at once")
            {
                // expected, noop
            } else {
                println!("{}", s);
            }
        }));

        let res = panic::catch_unwind(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {});
        });

        assert_err!(res);

        panic::set_hook(prev_hook);
    });
}

#[test]
fn after_start_and_before_stop_is_called() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let _ = env_logger::try_init();

    let after_start = Arc::new(AtomicUsize::new(0));
    let before_stop = Arc::new(AtomicUsize::new(0));

    let after_inner = after_start.clone();
    let before_inner = before_stop.clone();
    let rt = tokio::runtime::Builder::new()
        .after_start(move || {
            after_inner.clone().fetch_add(1, Ordering::Relaxed);
        })
        .before_stop(move || {
            before_inner.clone().fetch_add(1, Ordering::Relaxed);
        })
        .build()
        .unwrap();

    let (tx, rx) = mpsc::channel();

    rt.block_on(client_server(tx));

    let mut e = tokio_executor::enter().unwrap();
    e.block_on(rt.shutdown_on_idle());

    assert_ok!(rx.try_recv());

    assert!(after_start.load(Ordering::Relaxed) > 0);
    assert!(before_stop.load(Ordering::Relaxed) > 0);
}
