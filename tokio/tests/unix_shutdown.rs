#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", unix))] // Unix domain sockets are only available on Unix
#![cfg(not(miri))] // No Unix domain sockets on miri.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio_test::assert_ok;

#[tokio::test]
async fn shutdown() {
    let (mut a, mut b) = assert_ok!(UnixStream::pair());

    let handle = tokio::spawn(async move {
        assert_ok!(AsyncWriteExt::shutdown(&mut b).await);
    });

    // The peer observes the shutdown as a read of 0.
    let mut buf = [0u8; 1];
    let n = assert_ok!(a.read(&mut buf).await);
    assert_eq!(n, 0);

    handle.await.unwrap();
}

#[tokio::test]
async fn shutdown_after_peer_dropped() {
    let (mut stream, peer) = assert_ok!(UnixStream::pair());
    // "External event": the peer closes its half (e.g. process exit).
    drop(peer);

    // `shutdown` is a cleanup call: like the TCP side (#4665), it must not
    // surface a spurious `NotConnected` on platforms whose `shutdown(2)`
    // reports it after the peer is gone (e.g. macOS). See #8520.
    assert_ok!(AsyncWriteExt::shutdown(&mut stream).await);
    assert_ok!(AsyncWriteExt::shutdown(&mut stream).await);
}
