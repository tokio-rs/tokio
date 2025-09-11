use crate::{
    runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op},
    util::as_ref::OwnedBuf,
};
use io_uring::{opcode, types};
use std::{
    io,
    os::fd::{AsRawFd, OwnedFd},
};

#[derive(Debug)]
pub(crate) struct Write {
    buf: OwnedBuf,
    fd: OwnedFd,
}

impl Completable for Write {
    type Output = (u32, OwnedBuf, OwnedFd);
    fn complete(self, cqe: CqeResult) -> io::Result<Self::Output> {
        Ok((cqe.result?, self.buf, self.fd))
    }
}

impl Cancellable for Write {
    fn cancel(self) -> CancelData {
        CancelData::Write(self)
    }
}

impl Op<Write> {
    /// Issue a write that starts at `buf_offset` within `buf` and writes `len` bytes
    /// into `file` at `file_offset`.
    pub(crate) fn write_at(
        fd: OwnedFd,
        buf: OwnedBuf,
        buf_offset: usize,
        len: u32,
        file_offset: u64,
    ) -> io::Result<Self> {
        // Check if `buf_offset` stays in bounds of the allocation
        debug_assert!(buf_offset + len as usize <= buf.as_ref().len());

        // SAFETY:
        // - `buf_offset` stays in bounds of the allocation.
        // - `buf` is derived from an actual allocation, and the entire memory
        //    range is in bounds of that allocation.
        let ptr = unsafe { buf.as_ref().as_ptr().add(buf_offset) };

        let sqe = opcode::Write::new(types::Fd(fd.as_raw_fd()), ptr, len)
            .offset(file_offset)
            .build();

        // SAFETY: parameters of the entry, such as `fd` and `buf`, are valid
        // until this operation completes.
        let op = unsafe { Op::new(sqe, Write { buf, fd }) };
        Ok(op)
    }
}
