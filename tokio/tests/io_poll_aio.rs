#![warn(rust_2018_idioms)]
#![cfg(all(target_os = "freebsd", feature = "aio"))]

use futures_test::task::noop_context;
use mio_aio::{AioCb, AioFsyncMode};
use std::{
    os::unix::io::AsRawFd,
    task::Poll,
    time::{Duration, Instant},
};
use tempfile::tempdir;
use tokio::io::PollAio;
use tokio_test::{assert_pending, assert_ready_ok};

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .build()
        .unwrap()
}

/// aio_fsync is the simplest AIO operation.  This test ensures that Tokio can
/// register an aiocb, and set its readiness.
#[test]
fn fsync() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("testfile");
    let f = std::fs::File::create(&path).unwrap();
    let fd = f.as_raw_fd();
    let aiocb = AioCb::from_fd(fd, 0);
    let rt = rt();
    let mut poll_aio = {
        let _enter = rt.enter();
        PollAio::new_for_aio(aiocb).unwrap()
    };
    let mut ctx = noop_context();
    // Should be not ready after initialization
    assert_pending!(poll_aio.poll(&mut ctx));

    // Send the operation to the kernel
    (*poll_aio).fsync(AioFsyncMode::O_SYNC).unwrap();

    // Wait for completeness
    let start = Instant::now();
    while Instant::now() - start < Duration::new(5, 0) {
        if let Poll::Ready(_ev) = poll_aio.poll(&mut ctx) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_ready_ok!(poll_aio.poll(&mut ctx));

    // Free its in-kernel state, so Nix doesn't panic.
    let mut aiocb = poll_aio.into_inner();
    aiocb.aio_return().unwrap();
}
