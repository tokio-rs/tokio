use crate::{
    io::uring::utils::SharedFd,
    runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op},
    util::as_ref::OwnedBuf,
};
use io_uring::{opcode, types};
use std::{io, os::fd::AsRawFd};

// This holds a strong ref to the resources such as `fd`, `buf`,
// preventing these resources from being dropped while the operation is in-flight.
#[derive(Debug)]
pub(crate) struct Write {
    _fd: SharedFd,
    _buf: OwnedBuf,
}

impl Completable for Write {
    // The number of bytes written.
    type Output = u32;
    fn complete(self, cqe: CqeResult) -> io::Result<Self::Output> {
        cqe.result
    }
}

impl Cancellable for Write {
    fn cancel(self) -> CancelData {
        CancelData::Write(self)
    }
}

impl Op<Write> {
    pub(crate) fn write_at(fd: SharedFd, buf: OwnedBuf, offset: u64) -> io::Result<Self> {
        // There is a cap on how many bytes we can write in a single uring write operation.
        // ref: https://github.com/axboe/liburing/discussions/497
        let len: u32 = std::cmp::min(buf.len(), u32::MAX as usize) as u32;

        let sqe = opcode::Write::new(types::Fd(fd.as_raw_fd()), buf.as_ref().as_ptr(), len)
            .offset(offset)
            .build();

        // SAFETY: `fd` and `buf` are owned by this `Op`, ensuring that these params are valid
        // until `Op::drop` gets called.
        let op = unsafe { Op::new(sqe, Write { _buf: buf, _fd: fd }) };
        Ok(op)
    }
}
