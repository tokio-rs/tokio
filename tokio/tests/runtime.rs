extern crate env_logger;
extern crate futures;
extern crate tokio;

use futures::sync::oneshot;
use std::sync::{atomic, Arc, Mutex};
use std::thread;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::future::lazy;
use tokio::prelude::*;
use tokio::runtime::Runtime;

// this import is used in all child modules that have it in scope
// from importing super::*, but the compiler doesn't realise that
// and warns about it.
pub use futures::future::Executor;

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
        }
    };
}

fn create_client_server_future() -> Box<Future<Item = (), Error = ()> + Send> {
    let server = t!(TcpListener::bind(&"127.0.0.1:0".parse().unwrap()));
    let addr = t!(server.local_addr());
    let client = TcpStream::connect(&addr);

    let server = server
        .incoming()
        .take(1)
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

    let future = server.join(client).map(|_| ());
    Box::new(future)
}

#[test]
fn runtime_tokio_run() {
    let _ = env_logger::try_init();

    tokio::run(create_client_server_future());
}

#[test]
fn runtime_single_threaded() {
    let _ = env_logger::try_init();

    let mut runtime = tokio::runtime::current_thread::Runtime::new().unwrap();
    runtime.block_on(create_client_server_future()).unwrap();
    runtime.run().unwrap();
}

#[test]
fn runtime_single_threaded_block_on() {
    let _ = env_logger::try_init();

    tokio::runtime::current_thread::block_on_all(create_client_server_future()).unwrap();
}

mod runtime_single_threaded_block_on_all {
    use super::*;

    fn test<F>(spawn: F)
    where
        F: Fn(Box<Future<Item = (), Error = ()> + Send>),
    {
        let cnt = Arc::new(Mutex::new(0));
        let c = cnt.clone();

        let msg = tokio::runtime::current_thread::block_on_all(lazy(move || {
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
    fn spawn() {
        test(|f| {
            tokio::spawn(f);
        })
    }

    #[test]
    fn execute() {
        test(|f| {
            tokio::executor::DefaultExecutor::current()
                .execute(f)
                .unwrap();
        })
    }
}

mod runtime_single_threaded_racy {
    use super::*;
    fn test<F>(spawn: F)
    where
        F: Fn(tokio::runtime::current_thread::Handle, Box<Future<Item = (), Error = ()> + Send>),
    {
        let (trigger, exit) = futures::sync::oneshot::channel();
        let (handle_tx, handle_rx) = ::std::sync::mpsc::channel();
        let jh = ::std::thread::spawn(move || {
            let mut rt = tokio::runtime::current_thread::Runtime::new().unwrap();
            handle_tx.send(rt.handle()).unwrap();

            // don't exit until we are told to
            rt.block_on(exit.map_err(|_| ())).unwrap();

            // run until all spawned futures (incl. the "exit" signal future) have completed.
            rt.run().unwrap();
        });

        let (tx, rx) = futures::sync::oneshot::channel();

        let handle = handle_rx.recv().unwrap();
        spawn(
            handle,
            Box::new(futures::future::lazy(move || {
                tx.send(()).unwrap();
                Ok(())
            })),
        );

        // signal runtime thread to exit
        trigger.send(()).unwrap();

        // wait for runtime thread to exit
        jh.join().unwrap();

        assert_eq!(rx.wait().unwrap(), ());
    }

    #[test]
    fn spawn() {
        test(|handle, f| {
            handle.spawn(f).unwrap();
        })
    }

    #[test]
    fn execute() {
        test(|handle, f| {
            handle.execute(f).unwrap();
        })
    }
}

mod runtime_multi_threaded {
    use super::*;
    fn test<F>(spawn: F)
    where
        F: Fn(&mut Runtime) + Send + 'static,
    {
        let _ = env_logger::try_init();

        let mut runtime = tokio::runtime::Builder::new().build().unwrap();
        spawn(&mut runtime);
        runtime.shutdown_on_idle().wait().unwrap();
    }

    #[test]
    fn spawn() {
        test(|rt| {
            rt.spawn(create_client_server_future());
        });
    }

    #[test]
    fn execute() {
        test(|rt| {
            rt.executor()
                .execute(create_client_server_future())
                .unwrap();
        });
    }
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

mod from_block_on {
    use super::*;

    fn test<F>(spawn: F)
    where
        F: Fn(Box<Future<Item = (), Error = ()> + Send>) + Send + 'static,
    {
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

        runtime.shutdown_on_idle().wait().unwrap();
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

mod many {
    use super::*;

    const ITER: usize = 200;
    fn test<F>(spawn: F)
    where
        F: Fn(&mut Runtime, Box<Future<Item = (), Error = ()> + Send>),
    {
        let cnt = Arc::new(Mutex::new(0));
        let mut runtime = Runtime::new().unwrap();

        for _ in 0..ITER {
            let c = cnt.clone();
            spawn(
                &mut runtime,
                Box::new(lazy(move || {
                    {
                        let mut x = c.lock().unwrap();
                        *x = 1 + *x;
                    }
                    Ok::<(), ()>(())
                })),
            );
        }

        runtime.shutdown_on_idle().wait().unwrap();
        assert_eq!(ITER, *cnt.lock().unwrap());
    }

    #[test]
    fn spawn() {
        test(|rt, f| {
            rt.spawn(f);
        })
    }

    #[test]
    fn execute() {
        test(|rt, f| {
            rt.executor().execute(f).unwrap();
        })
    }
}

mod from_block_on_all {
    use super::*;

    fn test<F>(spawn: F)
    where
        F: Fn(Box<Future<Item = (), Error = ()> + Send>) + Send + 'static,
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
        F1: Fn(Box<Future<Item = (), Error = ()> + Send>) + Send + 'static,
        F2: Fn(Box<Future<Item = (), Error = ()> + Send>) + panic::UnwindSafe + Send + 'static,
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
            let mut rt = threadpool_new();
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
