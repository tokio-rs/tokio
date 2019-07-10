#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use tokio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tokio::timer::Delay;
use tokio_test::{assert_ok, assert_err};

use env_logger;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;

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

/*
mod from_block_on_all {
    use super::*;

    fn test<F>(spawn: F)
    where
        F: Fn(Box<dyn Future<Item = (), Error = ()> + Send>) + Send + 'static,
    {
        let cnt = Arc::new(Mutex::new(0));
        let c = cnt.clone();

        let runtime = Runtime::new().unwrap();
        let msg = runtime
            .block_on_all(lazy(move || {
                {
                    let mut x = c.lock().unwrap();
                    *x = 1 + *x;
                }

                // Spawn!
                spawn(Box::new(lazy(move || {
                    {
                        let mut x = c.lock().unwrap();
                        *x = 1 + *x;
                    }
                    Ok::<(), ()>(())
                })));

                Ok::<_, ()>("hello")
            }))
            .unwrap();

        assert_eq!(2, *cnt.lock().unwrap());
        assert_eq!(msg, "hello");
    }

    #[test]
    fn execute() {
        test(|f| {
            tokio::executor::DefaultExecutor::current()
                .execute(f)
                .unwrap();
        })
    }

    #[test]
    fn spawn() {
        test(|f| {
            tokio::spawn(f);
        })
    }
}

mod nested_enter {
    use super::*;
    use std::panic;
    use tokio::runtime::current_thread;

    fn test<F1, F2>(first: F1, nested: F2)
    where
        F1: Fn(Box<dyn Future<Item = (), Error = ()> + Send>) + Send + 'static,
        F2: Fn(Box<dyn Future<Item = (), Error = ()> + Send>) + panic::UnwindSafe + Send + 'static,
    {
        let panicked = Arc::new(Mutex::new(false));
        let panicked2 = panicked.clone();

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

        first(Box::new(lazy(move || {
            panic::catch_unwind(move || nested(Box::new(lazy(|| Ok::<(), ()>(())))))
                .expect_err("nested should panic");
            *panicked2.lock().unwrap() = true;
            Ok::<(), ()>(())
        })));

        panic::set_hook(prev_hook);

        assert!(
            *panicked.lock().unwrap(),
            "nested call should have panicked"
        );
    }

    fn threadpool_new() -> Runtime {
        Runtime::new().expect("rt new")
    }

    #[test]
    fn run_in_run() {
        test(tokio::run, tokio::run);
    }

    #[test]
    fn threadpool_block_on_in_run() {
        test(tokio::run, |fut| {
            let rt = threadpool_new();
            rt.block_on(fut).unwrap();
        });
    }

    #[test]
    fn threadpool_block_on_all_in_run() {
        test(tokio::run, |fut| {
            let rt = threadpool_new();
            rt.block_on_all(fut).unwrap();
        });
    }

    #[test]
    fn current_thread_block_on_all_in_run() {
        test(tokio::run, |fut| {
            current_thread::block_on_all(fut).unwrap();
        });
    }
}

#[test]
fn runtime_reactor_handle() {
    #![allow(deprecated)]

    use futures::Stream;
    use std::net::{TcpListener as StdListener, TcpStream as StdStream};

    let rt = Runtime::new().unwrap();

    let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
    let tk_listener = TcpListener::from_std(std_listener, rt.handle()).unwrap();

    let addr = tk_listener.local_addr().unwrap();

    // Spawn a thread since we are avoiding the runtime
    let th = thread::spawn(|| for _ in tk_listener.incoming().take(1).wait() {});

    let _ = StdStream::connect(&addr).unwrap();

    th.join().unwrap();
}

#[test]
fn after_start_and_before_stop_is_called() {
    let _ = env_logger::try_init();

    let after_start = Arc::new(atomic::AtomicUsize::new(0));
    let before_stop = Arc::new(atomic::AtomicUsize::new(0));

    let after_inner = after_start.clone();
    let before_inner = before_stop.clone();
    let runtime = tokio::runtime::Builder::new()
        .after_start(move || {
            after_inner.clone().fetch_add(1, atomic::Ordering::Relaxed);
        })
        .before_stop(move || {
            before_inner.clone().fetch_add(1, atomic::Ordering::Relaxed);
        })
        .build()
        .unwrap();

    runtime.block_on_all(create_client_server_future()).unwrap();

    assert!(after_start.load(atomic::Ordering::Relaxed) > 0);
    assert!(before_stop.load(atomic::Ordering::Relaxed) > 0);
}
*/
