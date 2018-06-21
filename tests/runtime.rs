extern crate tokio;
extern crate env_logger;
extern crate futures;

use futures::sync::oneshot;
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::io;
use tokio::net::{TcpStream, TcpListener};
use tokio::prelude::future::lazy;
use tokio::prelude::*;
use tokio::runtime::Runtime;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

fn create_client_server_future() -> Box<Future<Item=(), Error=()> + Send> {
    let server = t!(TcpListener::bind(&"127.0.0.1:0".parse().unwrap()));
    let addr = t!(server.local_addr());
    let client = TcpStream::connect(&addr);

    let server = server.incoming().take(1)
        .map_err(|e| panic!("accept err = {:?}", e))
        .for_each(|socket| {
            tokio::spawn({
                io::write_all(socket, b"hello")
                    .map(|_| ())
                    .map_err(|e| panic!("write err = {:?}", e))
            })
        })
        .map(|_| ());

    let client = client
        .map_err(|e| panic!("connect err = {:?}", e))
        .and_then(|client| {
            // Read all
            io::read_to_end(client, vec![])
                .map(|_| ())
                .map_err(|e| panic!("read err = {:?}", e))
        });

    let future = server.join(client)
        .map(|_| ());
    Box::new(future)
}

#[test]
fn runtime_tokio_run() {
    let _ = env_logger::init();

    tokio::run(create_client_server_future());
}

#[test]
fn runtime_single_threaded() {
    let _ = env_logger::init();

    let mut runtime = tokio::runtime::current_thread::Runtime::new()
        .unwrap();
    runtime.block_on(create_client_server_future()).unwrap();
    runtime.run().unwrap();
}

#[test]
fn runtime_multi_threaded() {
    let _ = env_logger::init();

    let mut runtime = tokio::runtime::Builder::new()
        .build()
        .unwrap();
    runtime.spawn(create_client_server_future());
    runtime.shutdown_on_idle().wait().unwrap();
}

#[test]
fn block_on_timer() {
    use std::time::{Duration, Instant};
    use tokio::timer::{Delay, Error};

    fn after_1s<T>(x: T) -> Box<Future<Item = T, Error = Error> + Send>
    where
        T: Send + 'static,
    {
        Box::new(Delay::new(Instant::now() + Duration::from_millis(100)).map(move |_| x))
    }

    let mut runtime = Runtime::new().unwrap();
    assert_eq!(runtime.block_on(after_1s(42)).unwrap(), 42);
    runtime.shutdown_on_idle().wait().unwrap();
}

#[test]
fn spawn_from_block_on() {
    let cnt = Arc::new(Mutex::new(0));
    let c = cnt.clone();

    let mut runtime = Runtime::new().unwrap();
    let msg = runtime
        .block_on(lazy(move || {
            {
                let mut x = c.lock().unwrap();
                *x = 1 + *x;
            }

            // Spawn!
            tokio::spawn(lazy(move || {
                {
                    let mut x = c.lock().unwrap();
                    *x = 1 + *x;
                }
                Ok::<(), ()>(())
            }));

            Ok::<_, ()>("hello")
        }))
        .unwrap();

    runtime.shutdown_on_idle().wait().unwrap();
    assert_eq!(2, *cnt.lock().unwrap());
    assert_eq!(msg, "hello");
}

#[test]
fn block_waits() {
    let (tx, rx) = oneshot::channel();

    thread::spawn(|| {
        use std::time::Duration;
        thread::sleep(Duration::from_millis(1000));
        tx.send(()).unwrap();
    });

    let cnt = Arc::new(Mutex::new(0));
    let c = cnt.clone();

    let mut runtime = Runtime::new().unwrap();
    runtime
        .block_on(rx.then(move |_| {
            {
                let mut x = c.lock().unwrap();
                *x = 1 + *x;
            }
            Ok::<_, ()>(())
        }))
        .unwrap();

    assert_eq!(1, *cnt.lock().unwrap());
    runtime.shutdown_on_idle().wait().unwrap();
}

#[test]
fn spawn_many() {
    const ITER: usize = 200;

    let cnt = Arc::new(Mutex::new(0));
    let mut runtime = Runtime::new().unwrap();

    for _ in 0..ITER {
        let c = cnt.clone();
        runtime.spawn(lazy(move || {
            {
                let mut x = c.lock().unwrap();
                *x = 1 + *x;
            }
            Ok::<(), ()>(())
        }));
    }

    runtime.shutdown_on_idle().wait().unwrap();
    assert_eq!(ITER, *cnt.lock().unwrap());
}

#[test]
fn spawn_from_block_on_all() {
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
            tokio::spawn(lazy(move || {
                {
                    let mut x = c.lock().unwrap();
                    *x = 1 + *x;
                }
                Ok::<(), ()>(())
            }));

            Ok::<_, ()>("hello")
        }))
        .unwrap();

    assert_eq!(2, *cnt.lock().unwrap());
    assert_eq!(msg, "hello");
}
