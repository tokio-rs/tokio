#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi"), not(miri)))] // Wasi doesn't support bind
                                                                   // No `socket` on miri.

use std::time::Duration;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::sleep;
use tokio_test::assert_ok;

#[tokio::test]
async fn shutdown() {
    let srv = assert_ok!(TcpListener::bind("127.0.0.1:0").await);
    let addr = assert_ok!(srv.local_addr());

    let handle = tokio::spawn(async move {
        let mut stream = assert_ok!(TcpStream::connect(&addr).await);

        assert_ok!(AsyncWriteExt::shutdown(&mut stream).await);

        let mut buf = [0u8; 1];
        let n = assert_ok!(stream.read(&mut buf).await);
        assert_eq!(n, 0);
    });

    let (mut stream, _) = assert_ok!(srv.accept().await);
    let (mut rd, mut wr) = stream.split();

    let n = assert_ok!(io::copy(&mut rd, &mut wr).await);
    assert_eq!(n, 0);
    assert_ok!(AsyncWriteExt::shutdown(&mut stream).await);
    handle.await.unwrap()
}

#[tokio::test]
async fn shutdown_after_tcp_reset() {
    let srv = assert_ok!(TcpListener::bind("127.0.0.1:0").await);
    let addr = assert_ok!(srv.local_addr());

    let handle = tokio::spawn(async move {
        let mut stream = assert_ok!(TcpStream::connect(&addr).await);
        sleep(Duration::from_millis(2)).await;
        assert_ok!(AsyncWriteExt::shutdown(&mut stream).await);
    });

    let (stream, _) = assert_ok!(srv.accept().await);
    // By setting linger to 0 we will trigger a TCP reset
    stream.set_linger(Some(Duration::new(0, 0))).unwrap();

    sleep(Duration::from_millis(1)).await;
    drop(stream);

    handle.await.unwrap();
}
