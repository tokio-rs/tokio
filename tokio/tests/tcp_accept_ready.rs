#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi"), not(miri)))]

//! An accepted socket starts out assumed readable and writable, so its first
//! read and write try the syscall instead of waiting for the driver's first
//! event.

use futures::FutureExt;
use std::future::Future;
use std::io::{IoSlice, Write};
use std::pin::pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, AsyncWriteExt, Interest};
use tokio::net::{TcpListener, TcpStream};

/// Waits until `n` bytes are queued on `s`, using a raw `MSG_PEEK` that does not touch
/// tokio's readiness, so a test never depends on loopback delivery timing.
fn wait_until_queued(s: &TcpStream, n: usize) {
    if n == 0 {
        return;
    }
    let sock = socket2::SockRef::from(s);
    let mut buf = [std::mem::MaybeUninit::<u8>::uninit(); 64];
    assert!(n <= buf.len());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !matches!(sock.peek(&mut buf), Ok(m) if m >= n) {
        assert!(
            std::time::Instant::now() < deadline,
            "peer's bytes never arrived"
        );
        std::thread::yield_now();
    }
}

async fn accepted_with(data: &[u8]) -> (TcpStream, std::net::TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client.write_all(data).unwrap();
    let (s, _) = listener.accept().await.unwrap();
    wait_until_queued(&s, data.len());
    (s, client)
}

#[tokio::test]
async fn accepted_stream_reads_without_waiting_for_an_event() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // The peer connects and sends before we accept, as a client racing an
    // overloaded server does.
    let mut client = std::net::TcpStream::connect(addr).unwrap();
    client.write_all(b"hello").unwrap();

    let (stream, _) = listener.accept().await.unwrap();
    wait_until_queued(&stream, 5);

    // `try_read` never waits for the driver; if the socket did not start out
    // readable it would return `WouldBlock` here despite the queued data.
    let mut buf = [0u8; 16];
    let n = stream
        .try_read(&mut buf)
        .expect("data was queued before the first read");
    assert_eq!(&buf[..n], b"hello");
    // The first write tries the socket too.
    assert_eq!(stream.try_write(b"world").unwrap(), 5);
}

#[tokio::test]
async fn accepted_stream_with_no_data_still_waits() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mut client = std::net::TcpStream::connect(addr).unwrap();
    let (stream, _) = listener.accept().await.unwrap();

    // Nothing queued at accept time: the socket is assumed readable (a false
    // positive) until a read finds nothing, and it is writable.
    assert!(stream.readable().now_or_never().is_some());
    let mut buf = [0u8; 16];
    let err = stream.try_read(&mut buf).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::WouldBlock);
    assert!(stream.readable().now_or_never().is_none());
    assert_eq!(stream.try_write(b"early").unwrap(), 5);
    // The normal event path still delivers data that arrives later.
    client.write_all(b"later").unwrap();
    stream.readable().await.unwrap();
    let n = stream.try_read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"later");
}

// The `AsyncRead`/`AsyncWrite` path (`PollEvented`), which HTTP stacks use.
#[tokio::test]
async fn first_poll_read_and_write_are_ready() {
    let (mut s, _c) = accepted_with(b"hello").await;
    let mut cx = Context::from_waker(futures::task::noop_waker_ref());
    let mut buf = [0u8; 16];
    assert!(matches!(
        pin!(s.read(&mut buf)).poll(&mut cx),
        Poll::Ready(Ok(5))
    ));
    assert!(matches!(
        pin!(s.write(b"x")).poll(&mut cx),
        Poll::Ready(Ok(1))
    ));
}

// A read that fills the buffer keeps the socket readable, so draining
// continues without an event.
#[tokio::test]
async fn full_first_read_keeps_draining() {
    let (mut s, _c) = accepted_with(b"helloworld").await;
    let mut cx = Context::from_waker(futures::task::noop_waker_ref());
    let mut buf = [0u8; 5];
    assert!(matches!(
        pin!(s.read(&mut buf)).poll(&mut cx),
        Poll::Ready(Ok(5))
    ));
    assert!(matches!(
        pin!(s.read(&mut buf)).poll(&mut cx),
        Poll::Ready(Ok(5))
    ));
}

// After the runtime is gone: later reads still report the driver gone rather
// than hang.
#[test]
fn later_reads_after_runtime_drop_report_shutdown() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (mut s, _c) = rt.block_on(accepted_with(b"hello"));
    drop(rt);
    assert_eq!(s.try_read(&mut [0u8; 5]).unwrap(), 5);
    // This read finds nothing and clears readiness after shutdown; the
    // shutdown bit must survive that or the read below would hang.
    let _ = s.try_read(&mut [0u8; 5]);
    let rt2 = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt2.block_on(async {
        let r = tokio::time::timeout(std::time::Duration::from_secs(1), s.read(&mut [0u8; 5]))
            .await
            .expect("read hung instead of reporting shutdown");
        assert!(tokio::runtime::is_rt_shutdown_err(&r.unwrap_err()));
    });
}

#[cfg(unix)]
#[tokio::test]
async fn unix_accepted_stream_reads_without_an_event() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let mut client = std::os::unix::net::UnixStream::connect(&path).unwrap();
    client.write_all(b"hello").unwrap();
    let (s, _) = listener.accept().await.unwrap();
    // Same-host UDS write is synchronous into the peer's buffer; peek to be sure.
    let sock = socket2::SockRef::from(&s);
    let mut peek = [std::mem::MaybeUninit::<u8>::uninit(); 8];
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !matches!(sock.peek(&mut peek), Ok(m) if m >= 5) {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    let mut buf = [0u8; 16];
    assert_eq!(s.try_read(&mut buf).unwrap(), 5);
}

// `peek` goes through `async_io`.
#[tokio::test]
async fn peek_first_poll_ready() {
    let (s, _c) = accepted_with(b"hello").await;
    let mut cx = Context::from_waker(futures::task::noop_waker_ref());
    let mut buf = [0u8; 16];
    let r = pin!(s.peek(&mut buf)).poll(&mut cx);
    assert!(matches!(r, Poll::Ready(Ok(5))), "{r:?}");
}

// `write_vectored` goes through `poll_write_io` / `poll_io`.
#[tokio::test]
async fn write_vectored_first_poll_ready() {
    let (mut s, _c) = accepted_with(b"").await;
    let mut cx = Context::from_waker(futures::task::noop_waker_ref());
    let bufs = [IoSlice::new(b"ab"), IoSlice::new(b"cd")];
    let r = pin!(s.write_vectored(&bufs)).poll(&mut cx);
    assert!(matches!(r, Poll::Ready(Ok(4))), "{r:?}");
}

// An `async_io` whose closure finds the assumed readiness wrong clears it,
// waits for the driver, and calls the closure again once data arrives.
#[tokio::test]
async fn async_io_wouldblock_falls_back_to_wait() {
    let (s, mut c) = accepted_with(b"").await;
    let calls = std::cell::Cell::new(0);
    let fut = s.async_io(Interest::READABLE, || {
        calls.set(calls.get() + 1);
        let mut b = [std::mem::MaybeUninit::<u8>::uninit(); 8];
        socket2::SockRef::from(&s).recv(&mut b)
    });
    let mut fut = pin!(fut);
    assert!(fut.as_mut().now_or_never().is_none());
    c.write_all(b"xyz").unwrap();
    let n = tokio::time::timeout(std::time::Duration::from_secs(5), fut)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(n, 3);
    assert!(calls.get() >= 2, "calls={}", calls.get());
}
