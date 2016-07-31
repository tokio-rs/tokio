extern crate futures;
extern crate futures_io;
extern crate futures_mio;

use std::net::TcpStream;
use std::thread;
use std::io::{Read, Write};

use futures::Future;
use futures::stream::Stream;
use futures_io::{copy, TaskIo};

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn echo_server() {
    let mut l = t!(futures_mio::Loop::new());
    let srv = l.handle().tcp_listen(&"127.0.0.1:0".parse().unwrap());
    let srv = t!(l.run(srv));
    let addr = t!(srv.local_addr());

    let msg = "foo bar baz";
    let t = thread::spawn(move || {
        let mut s = TcpStream::connect(&addr).unwrap();

        for _i in 0..1024 {
            assert_eq!(t!(s.write(msg.as_bytes())), msg.len());
            let mut buf = [0; 1024];
            assert_eq!(t!(s.read(&mut buf)), msg.len());
            assert_eq!(&buf[..msg.len()], msg.as_bytes());
        }
    });

    let clients = srv.incoming();
    let client = clients.into_future().map(|e| e.0.unwrap()).map_err(|e| e.0);
    let halves = client.and_then(|s| TaskIo::new(s.0)).map(|i| i.split());
    let copied = halves.and_then(|(a, b)| copy(a, b));

    let amt = t!(l.run(copied));
    t.join().unwrap();

    assert_eq!(amt, msg.len() as u64 * 1024);
}
