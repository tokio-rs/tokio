#![warn(rust_2018_idioms)]
#![cfg(any(
    feature = "full",
    all(
        target_os = "emscripten",
        feature = "rt",
        feature = "macros",
        feature = "io-util"
    )
))]

use std::io::IoSlice;
use tokio::io::{AsyncWrite, AsyncWriteExt};

#[tokio::test]
async fn sink_poll_write_is_cooperative() {
    tokio::select! {
        biased;
        _ = async {
            loop {
                let buf = vec![1, 2, 3];
                tokio::io::sink().write_all(&buf).await.unwrap();
            }
        } => {},
        _ = tokio::task::yield_now() => {}
    }
}

#[tokio::test]
async fn sink_write_vectored_consumes_all_buffers() {
    let mut sink = tokio::io::sink();
    assert!(sink.is_write_vectored());

    let bufs = [
        IoSlice::new(b""),
        IoSlice::new(b"hello"),
        IoSlice::new(b""),
        IoSlice::new(b"world"),
    ];
    assert_eq!(sink.write_vectored(&bufs).await.unwrap(), 10);
}

#[tokio::test]
async fn sink_poll_write_vectored_is_cooperative() {
    tokio::select! {
        biased;
        _ = async {
            let bufs = [IoSlice::new(b"hello"), IoSlice::new(b"world")];
            loop {
                assert_eq!(tokio::io::sink().write_vectored(&bufs).await.unwrap(), 10);
            }
        } => {},
        _ = tokio::task::yield_now() => {}
    }
}

#[tokio::test]
async fn sink_poll_flush_is_cooperative() {
    tokio::select! {
        biased;
        _ = async {
            loop {
                tokio::io::sink().flush().await.unwrap();
            }
        } => {},
        _ = tokio::task::yield_now() => {}
    }
}

#[tokio::test]
async fn sink_poll_shutdown_is_cooperative() {
    tokio::select! {
        biased;
        _ = async {
            loop {
                tokio::io::sink().shutdown().await.unwrap();
            }
        } => {},
        _ = tokio::task::yield_now() => {}
    }
}
