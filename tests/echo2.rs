#![cfg(feature = "unstable-futures")]

// This test is the same as `echo.rs`, but ported to futures 0.2

extern crate env_logger;
extern crate futures2;
extern crate tokio;
extern crate tokio_io;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;

use futures2::prelude::*;
use futures2::executor::block_on;
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
    let copied = halves.and_then(|(a, b)| a.copy_into(b));

    let (amt, _, _) = t!(block_on(copied));
    t.join().unwrap();

    assert_eq!(amt, msg.len() as u64 * 1024);
}
