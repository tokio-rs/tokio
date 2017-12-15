extern crate env_logger;
extern crate futures;
extern crate tokio;
extern crate tokio_io;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;

use futures::Future;
use futures::stream::Stream;
use tokio_io::io::copy;
use tokio_io::AsyncRead;
use tokio::net::TcpListener;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn echo_server() {
    drop(env_logger::init());

    let srv = t!(TcpListener::bind(&t!("127.0.0.1:0".parse())));
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

    t!(future.wait());

    t.join().unwrap();
}
