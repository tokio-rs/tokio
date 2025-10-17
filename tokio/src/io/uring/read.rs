use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};
use io_uring::{opcode, types};
use std::{
    io,
    os::fd::{AsRawFd, OwnedFd},
};

#[derive(Debug)]
pub(crate) struct Read {
    buf: Vec<u8>,
    fd: OwnedFd,
}

impl Completable for Read {
    type Output = (u32, Vec<u8>);
    fn complete(self, cqe: CqeResult) -> io::Result<Self::Output> {
        let res = cqe.result?;

        Ok((res, self.buf))
    }
}

impl Cancellable for Read {
    fn cancel(self) -> CancelData {
        CancelData::Read(self)
    }
}

impl Op<Read> {
    /// Submit a request to open a file.
    pub(crate) fn read(fd: OwnedFd, mut buf: Vec<u8>) -> io::Result<Self> {
        let buf_mut_ptr = buf.as_mut_ptr();
        let cap = buf.capacity() as _;

        let read_op = opcode::Read::new(types::Fd(fd.as_raw_fd()), buf_mut_ptr, cap).build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        let op = unsafe { Op::new(read_op, Read { fd, buf }) };

        Ok(op)
    }
}
