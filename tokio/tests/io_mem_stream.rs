#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use futures::FutureExt;
use std::io::IoSlice;
use tokio::io::{duplex, simplex, AsyncReadExt, AsyncWriteExt, SimplexStream};

#[test]
#[should_panic = "must be greater than 0"]
fn duplex_zero_capacity_panics() {
    let _ = duplex(0);
}

#[test]
#[should_panic = "must be greater than 0"]
fn simplex_zero_capacity_panics() {
    let _ = simplex(0);
}

#[test]
#[should_panic = "must be greater than 0"]
fn new_unsplit_zero_capacity_panics() {
    let _ = SimplexStream::new_unsplit(0);
}

#[tokio::test]
async fn ping_pong() {
    let (mut a, mut b) = duplex(32);

    let mut buf = [0u8; 4];

    a.write_all(b"ping").await.unwrap();
    b.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ping");

    b.write_all(b"pong").await.unwrap();
    a.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"pong");
}

#[tokio::test]
async fn across_tasks() {
    let (mut a, mut b) = duplex(32);

    let t1 = tokio::spawn(async move {
        a.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        a.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"pong");
    });

    let t2 = tokio::spawn(async move {
        let mut buf = [0u8; 4];
        b.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");
        b.write_all(b"pong").await.unwrap();
    });

    t1.await.unwrap();
    t2.await.unwrap();
}

#[tokio::test]
async fn disconnect() {
    let (mut a, mut b) = duplex(32);

    let t1 = tokio::spawn(async move {
        a.write_all(b"ping").await.unwrap();
        // and dropped
    });

    let t2 = tokio::spawn(async move {
        let mut buf = [0u8; 32];
        let n = b.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"ping");

        let n = b.read(&mut buf).await.unwrap();
        assert_eq!(n, 0);
    });

    t1.await.unwrap();
    t2.await.unwrap();
}

#[tokio::test]
async fn disconnect_reader() {
    let (a, mut b) = duplex(2);

    let t1 = tokio::spawn(async move {
        // this will block, as not all data fits into duplex
        b.write_all(b"ping").await.unwrap_err();
    });

    let t2 = tokio::spawn(async move {
        // here we drop the reader side, and we expect the writer in the other
        // task to exit with an error
        drop(a);
    });

    t2.await.unwrap();
    t1.await.unwrap();
}

#[tokio::test]
async fn max_write_size() {
    let (mut a, mut b) = duplex(32);

    let t1 = tokio::spawn(async move {
        let n = a.write(&[0u8; 64]).await.unwrap();
        assert_eq!(n, 32);
        let n = a.write(&[0u8; 64]).await.unwrap();
        assert_eq!(n, 4);
    });

    let mut buf = [0u8; 4];
    b.read_exact(&mut buf).await.unwrap();

    t1.await.unwrap();

    // drop b only after task t1 finishes writing
    drop(b);
}

#[tokio::test]
async fn zero_length_operations() {
    let (mut reader, _peer) = duplex(1);
    assert!(matches!(reader.read(&mut []).now_or_never(), Some(Ok(0))));

    let (mut writer, _peer) = duplex(1);
    writer.write_all(b"x").await.unwrap();
    assert!(matches!(writer.write(&[]).now_or_never(), Some(Ok(0))));

    let (mut writer, _peer) = duplex(1);
    writer.write_all(b"x").await.unwrap();
    let bufs = [IoSlice::new(&[]), IoSlice::new(&[])];
    assert!(matches!(
        writer.write_vectored(&bufs).now_or_never(),
        Some(Ok(0))
    ));
}

#[tokio::test]
async fn zero_length_writes_to_closed_stream() {
    let (mut writer, peer) = duplex(1);
    drop(peer);
    let err = writer.write(&[]).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe);

    let (mut writer, peer) = duplex(1);
    drop(peer);
    let bufs = [IoSlice::new(&[]), IoSlice::new(&[])];
    let err = writer.write_vectored(&bufs).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe);
}

#[tokio::test]
async fn duplex_is_cooperative() {
    let (mut tx, mut rx) = tokio::io::duplex(1024 * 8);

    tokio::select! {
        biased;

        _ = async {
            loop {
                let buf = [3u8; 4096];
                tx.write_all(&buf).await.unwrap();
                let mut buf = [0u8; 4096];
                let _ = rx.read(&mut buf).await.unwrap();
            }
        } => {},
        _ = tokio::task::yield_now() => {}
    }
}
