#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::vec::AsyncVectoredWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio_test::assert_ok;

use std::io::IoSlice;

#[tokio::test]
async fn echo_server() {
    const N: usize = 1024;

    let srv = assert_ok!(TcpListener::bind("127.0.0.1:0").await);
    let addr = assert_ok!(srv.local_addr());

    let msg = b"foo bar baz";

    let t = tokio::spawn(async move {
        let mut s = assert_ok!(TcpStream::connect(&addr).await);

        let t2 = tokio::spawn(async move {
            let mut s = assert_ok!(TcpStream::connect(&addr).await);
            let mut b = vec![0; msg.len() * N];
            assert_ok!(s.read_exact(&mut b).await);
            b
        });

        let mut expected = Vec::<u8>::new();
        for _i in 0..N {
            expected.extend(msg);
            let io_vec = [
                IoSlice::new(&msg[0..4]),
                IoSlice::new(&msg[4..8]),
                IoSlice::new(&msg[8..]),
            ];
            let res = assert_ok!(s.write_vectored(&io_vec).await);
            assert_eq!(res, msg.len());
        }

        (expected, t2)
    });

    let (mut a, _) = assert_ok!(srv.accept().await);
    let (mut b, _) = assert_ok!(srv.accept().await);

    let n = assert_ok!(io::copy(&mut a, &mut b).await);

    let (expected, t2) = t.await.unwrap();
    let actual = t2.await.unwrap();

    assert!(expected == actual);
    assert_eq!(n, (msg.len() * N) as u64);
}
