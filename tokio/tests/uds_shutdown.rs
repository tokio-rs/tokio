#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]
#![cfg(unix)]
#![cfg(not(miri))] // No Unix domain sockets on miri.

use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio_test::assert_ok;

/// On macOS, `shutdown(2)` on a Unix stream whose peer is already gone returns
/// `ENOTCONN`. `UnixStream::shutdown` must treat that as a successful cleanup,
/// matching `TcpStream` (`#4665`). On Linux the kernel already returns `Ok`.
#[tokio::test]
async fn shutdown_after_peer_dropped() {
    let (mut stream, peer) = UnixStream::pair().unwrap();
    drop(peer);

    assert_ok!(AsyncWriteExt::shutdown(&mut stream).await);
    // A second call must stay a no-op rather than surfacing `NotConnected`.
    assert_ok!(AsyncWriteExt::shutdown(&mut stream).await);
}

#[tokio::test]
async fn shutdown_split_write_half_after_peer_dropped() {
    let (stream, peer) = UnixStream::pair().unwrap();
    drop(peer);

    let (_rd, mut wr) = stream.into_split();
    assert_ok!(AsyncWriteExt::shutdown(&mut wr).await);
    assert_ok!(AsyncWriteExt::shutdown(&mut wr).await);
}
