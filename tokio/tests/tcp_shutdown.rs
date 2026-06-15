#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi")))] // Wasi doesn't support mulithreading

use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot::channel;
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
#[cfg_attr(miri, ignore = "Miri doesn't support `SO_LINGER`")]
async fn shutdown_after_tcp_reset() {
    let srv = assert_ok!(TcpListener::bind("127.0.0.1:0").await);
    let addr = assert_ok!(srv.local_addr());

    let (connected_tx, connected_rx) = channel();
    let (dropped_tx, dropped_rx) = channel();

    let handle = tokio::spawn(async move {
        let mut stream = assert_ok!(TcpStream::connect(&addr).await);
        connected_tx.send(()).unwrap();

        dropped_rx.await.unwrap();

        // After the peer's RST (linger = 0), `shutdown` returns `Ok(())` on most
        // platforms, but FreeBSD can surface the reset as `ConnectionReset` when
        // the kernel processed it before `shutdown` ran. Both are valid for an
        // already-reset connection.
        match AsyncWriteExt::shutdown(&mut stream).await {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::ConnectionReset => {}
            Err(e) => panic!("unexpected error after reset: {e:?}"),
        }
    });

    let (stream, _) = assert_ok!(srv.accept().await);
    // By setting linger to 0 we will trigger a TCP reset
    stream.set_zero_linger().unwrap();
    connected_rx.await.unwrap();

    drop(stream);
    dropped_tx.send(()).unwrap();

    handle.await.unwrap();
}

#[tokio::test]
async fn shutdown_multiple_calls() {
    let srv = assert_ok!(TcpListener::bind("127.0.0.1:0").await);
    let addr = assert_ok!(srv.local_addr());

    let (connected_tx, connected_rx) = channel();

    let handle = tokio::spawn(async move {
        let mut stream = assert_ok!(TcpStream::connect(&addr).await);
        connected_tx.send(()).unwrap();
        assert_ok!(AsyncWriteExt::shutdown(&mut stream).await);
        assert_ok!(AsyncWriteExt::shutdown(&mut stream).await);
        assert_ok!(AsyncWriteExt::shutdown(&mut stream).await);
    });

    let (mut stream, _) = assert_ok!(srv.accept().await);
    connected_rx.await.unwrap();

    assert_ok!(AsyncWriteExt::shutdown(&mut stream).await);
    handle.await.unwrap();
}
