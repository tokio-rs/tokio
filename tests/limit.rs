extern crate futures;
extern crate tokio;
extern crate tokio_io;

use std::net::TcpStream;
use std::thread;
use std::io::{Write, Read};

use futures::Future;
use futures::stream::Stream;
use tokio_io::io::read_to_end;
use tokio::net::TcpListener;
use tokio::reactor::Handle;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn limit() {
    let handle = Handle::default();
    let srv = t!(TcpListener::bind(&t!("127.0.0.1:0".parse()), &handle));
    let addr = t!(srv.local_addr());

    let t = thread::spawn(move || {
        let mut s1 = TcpStream::connect(&addr).unwrap();
        s1.write_all(b"foo bar baz").unwrap();
    });

    let clients = srv.incoming().map(|e| e.0).take(1);
    let copied = clients.collect().and_then(|clients| {
        let mut clients = clients.into_iter();
        let a = clients.next().unwrap();

        read_to_end(a.take(4), Vec::new())
    });

    let (_, data) = t!(copied.wait());
    t.join().unwrap();

    assert_eq!(data, b"foo ");
}
