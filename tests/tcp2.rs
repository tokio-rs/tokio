#![cfg(feature = "unstable-futures")]

// This test is the same as `tcp.rs`, but ported to futures 0.2

extern crate env_logger;
extern crate tokio;
extern crate mio;
extern crate futures2;

use std::{net, thread};
use std::sync::mpsc::channel;

use tokio::net::{TcpListener, TcpStream};
use futures2::executor::block_on;
use futures2::prelude::*;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn connect() {
    drop(env_logger::init());
    let srv = t!(net::TcpListener::bind("127.0.0.1:0"));
    let addr = t!(srv.local_addr());
    let t = thread::spawn(move || {
        t!(srv.accept()).0
    });

    let stream = TcpStream::connect(&addr);
    let mine = t!(block_on(stream));
    let theirs = t.join().unwrap();

    assert_eq!(t!(mine.local_addr()), t!(theirs.peer_addr()));
    assert_eq!(t!(theirs.local_addr()), t!(mine.peer_addr()));
}

#[test]
fn accept() {
    drop(env_logger::init());
    let srv = t!(TcpListener::bind(&t!("127.0.0.1:0".parse())));
    let addr = t!(srv.local_addr());

    let (tx, rx) = channel();
    let client = srv.incoming().map(move |t| {
        tx.send(()).unwrap();
        t
    }).next().map_err(|e| e.0);
    assert!(rx.try_recv().is_err());
    let t = thread::spawn(move || {
        net::TcpStream::connect(&addr).unwrap()
    });

    let (mine, _remaining) = t!(block_on(client));
    let mine = mine.unwrap();
    let theirs = t.join().unwrap();

    assert_eq!(t!(mine.local_addr()), t!(theirs.peer_addr()));
    assert_eq!(t!(theirs.local_addr()), t!(mine.peer_addr()));
}

#[test]
fn accept2() {
    drop(env_logger::init());
    let srv = t!(TcpListener::bind(&t!("127.0.0.1:0".parse())));
    let addr = t!(srv.local_addr());

    let t = thread::spawn(move || {
        net::TcpStream::connect(&addr).unwrap()
    });

    let (tx, rx) = channel();
    let client = srv.incoming().map(move |t| {
        tx.send(()).unwrap();
        t
    }).next().map_err(|e| e.0);
    assert!(rx.try_recv().is_err());

    let (mine, _remaining) = t!(block_on(client));
    mine.unwrap();
    t.join().unwrap();
}

#[cfg(unix)]
mod unix {
    use tokio::net::TcpStream;
    use tokio::prelude::*;

    use env_logger;
    use futures2::future;
    use futures2::executor::block_on;
    use futures2::io::AsyncRead;
    use mio::unix::UnixReady;

    use std::{net, thread};
    use std::time::Duration;

    #[test]
    fn poll_hup() {
        drop(env_logger::init());

        let srv = t!(net::TcpListener::bind("127.0.0.1:0"));
        let addr = t!(srv.local_addr());
        let t = thread::spawn(move || {
            let mut client = t!(srv.accept()).0;
            client.write(b"hello world").unwrap();
            thread::sleep(Duration::from_millis(200));
        });

        let mut stream = t!(block_on(TcpStream::connect(&addr)));

        // Poll for HUP before reading.
        block_on(future::poll_fn(|cx| {
            stream.poll_read_ready2(cx, UnixReady::hup().into())
        })).unwrap();

        // Same for write half
        block_on(future::poll_fn(|cx| {
            stream.poll_write_ready2(cx)
        })).unwrap();

        let mut buf = vec![0; 11];

        // Read the data
        block_on(future::poll_fn(|cx| {
            stream.poll_read(cx, &mut buf)
        })).unwrap();

        assert_eq!(b"hello world", &buf[..]);

        t.join().unwrap();
    }
}
