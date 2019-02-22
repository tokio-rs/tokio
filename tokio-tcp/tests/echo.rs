extern crate env_logger;
extern crate futures;
extern crate tokio_io;
extern crate tokio_tcp;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;

use futures::stream::Stream;
use futures::Future;
use tokio_io::io::copy;
use tokio_io::AsyncRead;
use tokio_tcp::TcpListener;

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
        }
    };
}

#[test]
fn echo_server() {
    drop(env_logger::try_init());

    let srv = t!(TcpListener::bind(&t!("127.0.0.1:0".parse())));
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
    let halves = client.map(|s| s.split());
    let copied = halves.and_then(|(a, b)| copy(a, b));

    let (amt, _, _) = t!(copied.wait());
    t.join().unwrap();

    assert_eq!(amt, msg.len() as u64 * 1024);
}
