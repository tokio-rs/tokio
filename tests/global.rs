extern crate futures;
extern crate tokio;
extern crate tokio_io;
extern crate env_logger;

use std::{io, thread};
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::atomic::Ordering::Relaxed;

use futures::prelude::*;
use futures::future::lazy;
use futures::sync::oneshot;
use tokio::net::{TcpStream, TcpListener};
use tokio::runtime::Runtime;
use tokio::timer::Delay;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn hammer_old() {
    let _ = env_logger::try_init();

    let threads = (0..10).map(|_| {
        thread::spawn(|| {
            let srv = t!(TcpListener::bind(&"127.0.0.1:0".parse().unwrap()));
            let addr = t!(srv.local_addr());
            let mine = TcpStream::connect(&addr);
            let theirs = srv.incoming().into_future()
                .map(|(s, _)| s.unwrap())
                .map_err(|(s, _)| s);
            let (mine, theirs) = t!(mine.join(theirs).wait());

            assert_eq!(t!(mine.local_addr()), t!(theirs.peer_addr()));
            assert_eq!(t!(theirs.local_addr()), t!(mine.peer_addr()));
        })
    }).collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
}

struct Rd(Arc<TcpStream>);
struct Wr(Arc<TcpStream>);

impl io::Read for Rd {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        <&TcpStream>::read(&mut &*self.0, dst)
    }
}

impl tokio_io::AsyncRead for Rd {
}

impl io::Write for Wr {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        <&TcpStream>::write(&mut &*self.0, src)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl tokio_io::AsyncWrite for Wr {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        Ok(().into())
    }
}

#[test]
fn hammer_split() {
    use tokio_io::io;

    const N: usize = 100;
    const ITER: usize = 10;

    let _ = env_logger::try_init();

    for _ in 0..ITER {
        let srv = t!(TcpListener::bind(&"127.0.0.1:0".parse().unwrap()));
        let addr = t!(srv.local_addr());

        let cnt = Arc::new(AtomicUsize::new(0));

        let mut rt = Runtime::new().unwrap();

        fn split(socket: TcpStream, cnt: Arc<AtomicUsize>) {
            let socket = Arc::new(socket);
            let rd = Rd(socket.clone());
            let wr = Wr(socket);

            let cnt2 = cnt.clone();

            let rd = io::read(rd, vec![0; 1])
                .map(move |_| {
                    cnt2.fetch_add(1, Relaxed);
                })
                .map_err(|e| panic!("read error = {:?}", e));

            let wr = io::write_all(wr, b"1")
                .map(move |_| {
                    cnt.fetch_add(1, Relaxed);
                })
                .map_err(move |e| panic!("write error = {:?}", e));

            tokio::spawn(rd);
            tokio::spawn(wr);
        }

        rt.spawn({
            let cnt = cnt.clone();
            srv.incoming()
                .map_err(|e| panic!("accept error = {:?}", e))
                .take(N as u64)
                .for_each(move |socket| {
                    split(socket, cnt.clone());
                    Ok(())
                })
        });

        for _ in 0..N {
            rt.spawn({
                let cnt = cnt.clone();
                TcpStream::connect(&addr)
                    .map_err(move |e| panic!("connect error = {:?}", e))
                    .map(move |socket| split(socket, cnt))
            });
        }

        rt.shutdown_on_idle().wait().unwrap();
        assert_eq!(N * 4, cnt.load(Relaxed));
    }
}

#[test]
fn spawn_handle() {
    let f = lazy(|| {
        Ok::<_, ()>("hello from the future")
    });

    let res = tokio::run(lazy(move || {
        tokio::spawn_handle(f)
            .then(|res| {
                assert_eq!(res.unwrap(), "hello from the future");
                Ok(())
            })
    }));

}

#[test]
fn spawn_handle_error() {
    let f = lazy(|| {
        Err::<(), _>("something is wrong")
    });

    tokio::run(lazy(move || {
        tokio::spawn_handle(f)
            .or_else(|e| {
                assert_eq!(e.into_inner(), Some("something is wrong"));
                Ok::<_, ()>(())
            })
    }));
}

#[test]
fn spawn_handle_across_threads() {
    let (tx, rx) = oneshot::channel();

    let join = thread::spawn(move || {;
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let f = lazy(|| {
            Ok::<_, ()>("hello from the future in the other thread")
        });

        rt.block_on(lazy(move || {
            let handle = tokio::spawn_handle(f);
            tx.send(handle);
            Ok::<(), ()>(())
        })).unwrap();

        rt.shutdown_on_idle().wait().unwrap();
    });

    tokio::run(rx
        .map_err(|e| panic!("{:?}", e))
        .and_then(|handle| handle)
        .then(|res| {
            assert_eq!(res.unwrap(), "hello from the future in the other thread");
            Ok(())
        })
    );

    join.join().unwrap();
}

#[test]
fn spawn_handle_cancel() {
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let done2 = done.clone();

    let f = Delay::new(Instant::now() + Duration::from_millis(500))
        .then(move |_| {
            done2.store(true, Relaxed);
            Ok::<_, ()>(())
        });

    rt.block_on(lazy(move || {
        let handle = tokio::spawn_handle(f);
        handle.cancel();
        Ok::<(), ()>(())
    })).unwrap();

    rt.shutdown_on_idle().wait().unwrap();
    assert_eq!(done.load(Relaxed), false);
}
