use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};
use io_uring::{opcode, types};
use std::{
    cmp, io,
    os::fd::{AsRawFd, BorrowedFd},
};

#[derive(Debug)]
pub(crate) struct Write;

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
    pub(crate) fn write_at(fd: BorrowedFd<'_>, buf: &[u8], offset: u64) -> io::Result<Self> {
        // There is a cap on how many bytes we can write in a single uring write operation.
        // ref: https://github.com/axboe/liburing/discussions/497
        let len: u32 = cmp::min(buf.len(), u32::MAX as usize) as u32;

        let sqe = opcode::Write::new(types::Fd(fd.as_raw_fd()), buf.as_ptr(), len)
            .offset(offset)
            .build();

        // SAFETY: `fd` and `buf` are owned by caller function, ensuring these params are
        // valid until operation completes.
        let op = unsafe { Op::new(sqe, Write) };
        Ok(op)
    }
}
