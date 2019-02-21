extern crate env_logger;
extern crate futures;
extern crate tokio;
extern crate tokio_io;

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::{io, thread};

use futures::prelude::*;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
        }
    };
}

#[test]
fn hammer_old() {
    let _ = env_logger::try_init();

    let threads = (0..10)
        .map(|_| {
            thread::spawn(|| {
                let srv = t!(TcpListener::bind(&"127.0.0.1:0".parse().unwrap()));
                let addr = t!(srv.local_addr());
                let mine = TcpStream::connect(&addr);
                let theirs = srv
                    .incoming()
                    .into_future()
                    .map(|(s, _)| s.unwrap())
                    .map_err(|(s, _)| s);
                let (mine, theirs) = t!(mine.join(theirs).wait());

                assert_eq!(t!(mine.local_addr()), t!(theirs.peer_addr()));
                assert_eq!(t!(theirs.local_addr()), t!(mine.peer_addr()));
            })
        })
        .collect::<Vec<_>>();
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

impl tokio_io::AsyncRead for Rd {}

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
