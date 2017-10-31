extern crate env_logger;
extern crate futures;
extern crate tokio;

use std::net;
use std::sync::mpsc::channel;
use std::thread;

<<<<<<< 822d9f84453ac74646d1b8d8c87395f75bcb7604
use futures::Future;
use futures::stream::Stream;
use tokio::reactor::Core;
=======
use futures::prelude::*;
use futures::thread::block_until;
>>>>>>> Update as the `tokio` crate
use tokio::net::{TcpListener, TcpStream};

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
    let mine = t!(block_until(stream));
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
        t.0
    }).into_future().map_err(|e| e.0);
    assert!(rx.try_recv().is_err());
    let t = thread::spawn(move || {
        net::TcpStream::connect(&addr).unwrap()
    });

    let (mine, _remaining) = t!(block_until(client));
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
        t.0
    }).into_future().map_err(|e| e.0);
    assert!(rx.try_recv().is_err());

    let (mine, _remaining) = t!(block_until(client));
    mine.unwrap();
    t.join().unwrap();
}
