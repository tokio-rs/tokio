#![warn(rust_2018_idioms)]
#![cfg(all(target_os = "freebsd", feature = "aio"))]

use mio_aio::{AioCb, AioFsyncMode, LioCb};
use std::{
    future::Future,
    os::unix::io::{AsRawFd, RawFd},
    pin::Pin,
    task::{Context, Poll},
};
use tempfile::tempfile;
use tokio::{
    io::{AioSource, PollAio},
};

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

    /// A very crude implementation of an AIO-based future
    struct FsyncFut(PollAio<WrappedAioCb<'static>>);

    impl Future for FsyncFut {
        type Output = Result<(), std::io::Error>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>)
            -> Poll<Self::Output>
        {
            let poll_result = self.0.poll(cx);
            match poll_result {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Ready(Ok(_ev)) => {
                    // At this point, we could clear readiness.  But there's no
                    // point, since we're about to drop the PollAio.
                    let result = (*self.0).0.aio_return();
                    match result {
                        Ok(_) => Poll::Ready(Ok(())),
                        Err(e) => Poll::Ready(Err(e.into()))
                    }
                },
            }
        }
    }

    #[tokio::test]
    async fn fsync() {
        let f = tempfile().unwrap();
        let fd = f.as_raw_fd();
        let aiocb = AioCb::from_fd(fd, 0);
        let source = WrappedAioCb(aiocb);
        let mut poll_aio = PollAio::new_for_aio(source).unwrap();
        (*poll_aio).0.fsync(AioFsyncMode::O_SYNC).unwrap();
        let fut = FsyncFut(poll_aio);
        fut.await.unwrap();
    }
}

mod lio {
    use super::*;

    /// A very crude lio_listio-based Future
    struct LioFut(Option<PollAio<WrappedLioCb<'static>>>);

    impl Future for LioFut {
        type Output = Result<Vec<isize>, std::io::Error>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>)
            -> Poll<Self::Output>
        {
            let poll_result = self.0.as_mut().unwrap().poll(cx);
            match poll_result {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Ready(Ok(_ev)) => {
                    // At this point, we could clear readiness.  But there's no
                    // point, since we're about to drop the PollAio.
                    let r = self.0.take().unwrap().into_inner().0
                        .into_results(|iter| {
                            iter.map(|lr| lr.result.unwrap())
                            .collect::<Vec<isize>>()
                        });
                    Poll::Ready(Ok(r))
                }
            }
        }
    }

    /// An lio_listio operation with one write element
    #[tokio::test]
    async fn onewrite() {
        const WBUF: &[u8] = b"abcdef";
        let f = tempfile().unwrap();

        let mut builder = mio_aio::LioCbBuilder::with_capacity(1);
        builder = builder.emplace_slice(
            f.as_raw_fd(),
            0,
            &WBUF[..],
            0,
            mio_aio::LioOpcode::LIO_WRITE,
        );
        let liocb = builder.finish();
        let source = WrappedLioCb(liocb);
        let mut poll_aio = PollAio::new_for_lio(source).unwrap();

        // Send the operation to the kernel
        (*poll_aio).0.submit().unwrap();
        let fut = LioFut(Some(poll_aio));
        let v = fut.await.unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0] as usize, WBUF.len());
    }
}
