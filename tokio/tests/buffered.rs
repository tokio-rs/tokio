#![feature(async_await)]
#![warn(rust_2018_idioms)]
#![cfg(feature = "default")]

use tokio::net::TcpListener;
use tokio::prelude::*;
use tokio_test::assert_ok;

use std::io::prelude::*;
use std::net::TcpStream;
use std::thread;

#[tokio::test]
async fn echo_server() {
    const N: usize = 1024;

    let addr = assert_ok!("127.0.0.1:0".parse());
    let mut srv = assert_ok!(TcpListener::bind(&addr));
    let addr = assert_ok!(srv.local_addr());

    let msg = "foo bar baz";

    let t = thread::spawn(move || {
        let mut s = assert_ok!(TcpStream::connect(&addr));

        let t2 = thread::spawn(move || {
            let mut s = assert_ok!(TcpStream::connect(&addr));
            let mut b = vec![0; msg.len() * N];
            assert_ok!(s.read_exact(&mut b));
            b
        });

        let mut expected = Vec::<u8>::new();
        for _i in 0..N {
            expected.extend(msg.as_bytes());
            let res = assert_ok!(s.write(msg.as_bytes()));
            assert_eq!(res, msg.len());
        }

        (expected, t2)
    });

    let (mut a, _) = assert_ok!(srv.accept().await);
    let (mut b, _) = assert_ok!(srv.accept().await);

    let n = assert_ok!(a.copy(&mut b).await);

    let (expected, t2) = t.join().unwrap();
    let actual = t2.join().unwrap();

    assert!(expected == actual);
    assert_eq!(n, msg.len() as u64 * 1024);
}
