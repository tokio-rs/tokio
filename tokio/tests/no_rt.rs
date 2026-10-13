#![cfg(feature = "rt")]

#[test]
#[cfg(all(feature = "sync", feature = "time"))]
#[should_panic(
    expected = "there is no reactor running, must be called from the context of a Tokio 1.x runtime"
)]
fn timeout_panics_when_no_tokio_context() {
    use futures::executor::block_on;
    use tokio::sync::oneshot;
    use tokio::time::{timeout, Duration};

    let (_tx, rx) = oneshot::channel::<()>();
    let dur = Duration::from_millis(10);
    let _ = block_on(timeout(dur, rx));
}

#[test]
#[cfg(feature = "net")]
#[should_panic(
    expected = "there is no reactor running, must be called from the context of a Tokio 1.x runtime"
)]
fn panics_when_no_reactor() {
    use futures::executor::block_on;
    use std::net::TcpListener;
    use tokio::net::TcpStream;

    let srv = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = srv.local_addr().unwrap();
    block_on(TcpStream::connect(&addr)).unwrap();
}

#[test]
#[cfg(feature = "net")]
#[should_panic(
    expected = "there is no reactor running, must be called from the context of a Tokio 1.x runtime"
)]
fn io_panics_when_no_tokio_context() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();

    listener.set_nonblocking(true).unwrap();

    let _ = tokio::net::TcpListener::from_std(listener);
}

#[test]
#[cfg(feature = "fs")]
#[should_panic(
    expected = "there is no reactor running, must be called from the context of a Tokio 1.x runtime"
)]
fn fs_panics_when_no_tokio_context() {
    use futures::executor::block_on;

    let temp = tempfile::tempdir().unwrap();
    let _ = block_on(tokio::fs::read(temp.path().join("bar")));
}

#[test]
#[cfg(all(feature = "io-std", feature = "io-util"))]
#[should_panic(
    expected = "there is no reactor running, must be called from the context of a Tokio 1.x runtime"
)]
fn stdout_panics_when_no_tokio_context() {
    use futures::executor::block_on;
    use tokio::io::AsyncWriteExt;

    let mut out = tokio::io::stdout();
    let _ = block_on(out.write_all(b"hello\n"));
}
