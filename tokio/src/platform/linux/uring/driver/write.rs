use crate::platform::linux::uring::{
    buf::IoBuf,
    driver::{Op, SharedFd},
    BufResult,
};
use std::{
    io,
    task::{Context, Poll},
};

pub(crate) struct Write<T> {
    /// Holds a strong ref to the FD, preventing the file from being closed
    /// while the operation is in-flight.
    #[allow(dead_code)]
    fd: SharedFd,

    pub(crate) buf: T,
}

impl<T: IoBuf> Op<Write<T>> {
    pub(crate) fn write_at(fd: &SharedFd, buf: T, offset: u64) -> io::Result<Op<Write<T>>> {
        use io_uring::{opcode, types};

        Op::submit_with(
            Write {
                fd: fd.clone(),
                buf,
            },
            |write| {
                // Get raw buffer info
                let ptr = write.buf.stable_ptr();
                let len = write.buf.bytes_init();

                opcode::Write::new(types::Fd(fd.raw_fd()), ptr, len as _)
                    .offset(offset as _)
                    .build()
            },
        )
    }

    pub(crate) async fn write(mut self) -> BufResult<usize, T> {
        use crate::future::poll_fn;

        poll_fn(move |cx| self.poll_write(cx)).await
    }

    pub(crate) fn poll_write(&mut self, cx: &mut Context<'_>) -> Poll<BufResult<usize, T>> {
        use std::future::Future;
        use std::pin::Pin;

        let complete = ready!(Pin::new(self).poll(cx));
        Poll::Ready((complete.result.map(|v| v as _), complete.data.buf))
    }
}
