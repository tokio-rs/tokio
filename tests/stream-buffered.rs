extern crate futures;
extern crate tokio_core;
extern crate env_logger;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;

use futures::Future;
use futures::stream::Stream;
use tokio_core::io::{Io, copy};
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn echo_server() {
    drop(env_logger::init());

    let mut l = t!(Core::new());
    let srv = TcpListener::bind(&"127.0.0.1:0".parse().unwrap(), &l.handle());
    let srv = t!(l.run(srv));
    let addr = t!(srv.local_addr());

    let t = thread::spawn(move || {
        let mut s1 = t!(TcpStream::connect(&addr));
        let mut s2 = t!(TcpStream::connect(&addr));

        let msg = b"foo";
        assert_eq!(t!(s1.write(msg)), msg.len());
        assert_eq!(t!(s2.write(msg)), msg.len());
        let mut buf = [0; 1024];
        assert_eq!(t!(s1.read(&mut buf)), msg.len());
        assert_eq!(&buf[..msg.len()], msg);
        assert_eq!(t!(s2.read(&mut buf)), msg.len());
        assert_eq!(&buf[..msg.len()], msg);
    });

    let future = srv.incoming()
                    .map(|s| s.0.split())
                    .map(|(a, b)| copy(a, b).map(|_| ()))
                    .buffered(10)
                    .take(2)
                    .collect();

    t!(l.run(future));

    t.join().unwrap();
}
