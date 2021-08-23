#![warn(rust_2018_idioms)]
#![cfg(all(target_os = "freebsd", feature = "aio"))]

use futures_test::task::noop_context;
use mio_aio::{AioCb, AioFsyncMode, LioCb};
use std::{
    io::{Read, Seek, SeekFrom},
    os::unix::io::{AsRawFd, RawFd},
    task::Poll,
    time::{Duration, Instant},
};
use tempfile::tempdir;
use tokio::io::{AioSource, PollAio};
use tokio_test::{assert_pending, assert_ready_ok};

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .build()
        .unwrap()
}

struct WrappedAioCb<'a>(AioCb<'a>);
impl<'a> AioSource for WrappedAioCb<'a> {
    fn register(&mut self, kq: RawFd, token: usize) {
        self.0.register_raw(kq, token)
    }
    fn deregister(&mut self) {
        self.0.deregister_raw()
    }
}

struct WrappedLioCb<'a>(LioCb<'a>);
impl<'a> AioSource for WrappedLioCb<'a> {
    fn register(&mut self, kq: RawFd, token: usize) {
        self.0.register_raw(kq, token)
    }
    fn deregister(&mut self) {
        self.0.deregister_raw()
    }
}

mod aio {
    use super::*;

    /// aio_fsync is the simplest AIO operation.  This test ensures that Tokio
    /// can register an aiocb, and set its readiness.
    #[test]
    fn fsync() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("testfile");
        let f = std::fs::File::create(&path).unwrap();
        let fd = f.as_raw_fd();
        let aiocb = AioCb::from_fd(fd, 0);
        let source = WrappedAioCb(aiocb);
        let rt = rt();
        let mut poll_aio = {
            let _enter = rt.enter();
            PollAio::new_for_aio(source).unwrap()
        };
        let mut ctx = noop_context();
        // Should be not ready after initialization
        assert_pending!(poll_aio.poll(&mut ctx));

        // Send the operation to the kernel
        (*poll_aio).0.fsync(AioFsyncMode::O_SYNC).unwrap();

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
        let mut aiocb = poll_aio.into_inner().0;
        aiocb.aio_return().unwrap();
    }
}

mod lio {
    use super::*;

    /// An lio_listio operation with one write element
    #[test]
    fn onewrite() {
        let wbuf = b"abcdef";
        let dir = tempdir().unwrap();
        let path = dir.path().join("testfile");
        let mut f = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        let mut rbuf = Vec::new();

        let mut builder = mio_aio::LioCbBuilder::with_capacity(1);
        builder = builder.emplace_slice(
            f.as_raw_fd(),
            0,
            &wbuf[..],
            0,
            mio_aio::LioOpcode::LIO_WRITE,
        );
        let liocb = builder.finish();
        let source = WrappedLioCb(liocb);
        let rt = rt();
        let mut poll_aio = {
            let _enter = rt.enter();
            PollAio::new_for_lio(source).unwrap()
        };
        let mut ctx = noop_context();
        // Should be not ready after initialization
        assert_pending!(poll_aio.poll(&mut ctx));

        // Send the operation to the kernel
        (*poll_aio).0.submit().unwrap();

        // Wait for completeness
        let start = Instant::now();
        while Instant::now() - start < Duration::new(5, 0) {
            if let Poll::Ready(_ev) = poll_aio.poll(&mut ctx) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_ready_ok!(poll_aio.poll(&mut ctx));
        let liocb = poll_aio.into_inner().0;

        // Check results
        liocb.into_results(|mut iter| {
            let lr = iter.next().unwrap();
            assert_eq!(lr.result.unwrap(), wbuf.len() as isize);
            assert!(iter.next().is_none());
        });

        // Verify that it actually wrote to the file
        f.seek(SeekFrom::Start(0)).unwrap();
        let len = f.read_to_end(&mut rbuf).unwrap();
        assert_eq!(len, wbuf.len());
        assert_eq!(&wbuf[..], &rbuf[..]);
    }
}
