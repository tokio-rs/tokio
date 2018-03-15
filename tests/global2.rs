#![cfg(feature = "unstable-futures")]

// This test is the same as `global.rs`, but ported to futures 0.2

extern crate futures;
extern crate futures2;
extern crate tokio;
extern crate tokio_io;
extern crate env_logger;

use std::{io, thread};
use std::sync::Arc;

use futures2::prelude::*;
use futures2::executor::block_on;
use futures2::task;

use tokio::net::{TcpStream, TcpListener};
use tokio::runtime::Runtime;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn hammer() {
    let _ = env_logger::init();

    let threads = (0..10).map(|_| {
        thread::spawn(|| {
            let srv = t!(TcpListener::bind(&"127.0.0.1:0".parse().unwrap()));
            let addr = t!(srv.local_addr());
            let mine = TcpStream::connect(&addr);
            let theirs = srv.incoming().into_future()
                .map(|(s, _)| s.unwrap())
                .map_err(|(s, _)| s);
            let (mine, theirs) = t!(block_on(mine.join(theirs)));

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

impl AsyncRead for Rd {
    fn poll_read(&mut self, cx: &mut task::Context, dst: &mut [u8]) -> Poll<usize, io::Error> {
        <&TcpStream>::poll_read(&mut &*self.0, cx, dst)
    }
}

impl AsyncWrite for Wr {
    fn poll_write(&mut self, cx: &mut task::Context, src: &[u8]) -> Poll<usize, io::Error> {
        <&TcpStream>::poll_write(&mut &*self.0, cx, src)
    }

    fn poll_flush(&mut self, _cx: &mut task::Context) -> Poll<(), io::Error> {
        Ok(().into())
    }

    fn poll_close(&mut self, _cx: &mut task::Context) -> Poll<(), io::Error> {
        Ok(().into())
    }
}

#[test]
fn hammer_split() {
    const N: usize = 100;

    let _ = env_logger::init();

    let srv = t!(TcpListener::bind(&"127.0.0.1:0".parse().unwrap()));
    let addr = t!(srv.local_addr());

    let mut rt = Runtime::new().unwrap();

    fn split(socket: TcpStream) {
        let socket = Arc::new(socket);
        let rd = Rd(socket.clone());
        let wr = Wr(socket);

        let rd = rd.read(vec![0; 1])
            .map(|_| ())
            .map_err(|e| panic!("read error = {:?}", e));

        let wr = wr.write_all(b"1")
            .map(|_| ())
            .map_err(|e| panic!("write error = {:?}", e));

        tokio::spawn2(rd);
        tokio::spawn2(wr);
    }

    rt.spawn2({
        srv.incoming()
            .map_err(|e| panic!("accept error = {:?}", e))
            .take(N as u64)
            .for_each(|socket| {
                split(socket);
                Ok(())
            })
            .map(|_| ())
    });

    for _ in 0..N {
        rt.spawn2({
            TcpStream::connect(&addr)
                .map_err(|e| panic!("connect error = {:?}", e))
                .map(|socket| split(socket))
        });
    }

    futures::Future::wait(rt.shutdown_on_idle()).unwrap();
}
