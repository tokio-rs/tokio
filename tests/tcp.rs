extern crate env_logger;
extern crate tokio;
extern crate mio;
extern crate futures;

use std::{net, thread};
use std::sync::mpsc::channel;

use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;


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
    let mine = t!(stream.wait());
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
    }).into_future().map_err(|e| e.0);
    assert!(rx.try_recv().is_err());
    let t = thread::spawn(move || {
        net::TcpStream::connect(&addr).unwrap()
    });

    let (mine, _remaining) = t!(client.wait());
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
    }).into_future().map_err(|e| e.0);
    assert!(rx.try_recv().is_err());

    let (mine, _remaining) = t!(client.wait());
    mine.unwrap();
    t.join().unwrap();
}

#[cfg(unix)]
mod unix {
    use tokio::net::TcpStream;
    use tokio::prelude::*;

    use env_logger;
    use futures::future;
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

        let mut stream = t!(TcpStream::connect(&addr).wait());

        // Poll for HUP before reading.
        future::poll_fn(|| {
            stream.poll_read_ready(UnixReady::hup().into())
        }).wait().unwrap();

        // Same for write half
        future::poll_fn(|| {
            stream.poll_write_ready()
        }).wait().unwrap();

        let mut buf = vec![0; 11];

        // Read the data
        future::poll_fn(|| {
            stream.poll_read(&mut buf)
        }).wait().unwrap();

        assert_eq!(b"hello world", &buf[..]);

        t.join().unwrap();
    }
}
