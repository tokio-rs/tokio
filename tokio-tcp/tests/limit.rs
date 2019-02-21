extern crate futures;
extern crate tokio_io;
extern crate tokio_tcp;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;

use futures::stream::Stream;
use futures::Future;
use tokio_io::io::read_to_end;
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
fn limit() {
    let srv = t!(TcpListener::bind(&t!("127.0.0.1:0".parse())));
    let addr = t!(srv.local_addr());

    let t = thread::spawn(move || {
        let mut s1 = TcpStream::connect(&addr).unwrap();
        s1.write_all(b"foo bar baz").unwrap();
    });

    let clients = srv.incoming().take(1);
    let copied = clients.collect().and_then(|clients| {
        let mut clients = clients.into_iter();
        let a = clients.next().unwrap();

        read_to_end(a.take(4), Vec::new())
    });

    let (_, data) = t!(copied.wait());
    t.join().unwrap();

    assert_eq!(data, b"foo ");
}
