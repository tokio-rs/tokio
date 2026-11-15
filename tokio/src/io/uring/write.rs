use crate::io::uring::utils::UringFd;
use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};
use crate::util::as_ref::OwnedBuf;

use io_uring::squeue::Entry;
use io_uring::{opcode, types};
use std::fmt;
use std::io::{self, Error};
use std::os::fd::OwnedFd;

pub(crate) struct Write<F = OwnedFd> {
    buf: OwnedBuf,
    fd: F,
}

impl<F: UringFd> Write<F> {
    pub(crate) fn new(fd: F, buf: OwnedBuf) -> Self {
        Write { buf, fd }
    }

    /// Builds the entry that writes the buffer, starting at `buf_offset`, to
    /// the file at `file_offset`.
    ///
    /// `file_offset` is the file offset; use `u64::MAX` for the current cursor.
    ///
    /// The entry points into the heap memory of the buffer, which doesn't move
    /// when `self` does, so it stays valid until `self` is dropped.
    pub(crate) fn sqe(&self, buf_offset: usize, file_offset: u64) -> Entry {
        // There is a cap on how many bytes we can write in a single uring write operation.
        // ref: https://github.com/axboe/liburing/discussions/497
        let len = u32::try_from(self.buf.as_ref().len() - buf_offset).unwrap_or(u32::MAX);

        let ptr = self.buf.as_ref()[buf_offset..buf_offset + len as usize].as_ptr();

        opcode::Write::new(types::Fd(UringFd::as_raw_fd(&self.fd)), ptr, len)
            .offset(file_offset)
            .build()
    }
}

impl<F> fmt::Debug for Write<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Write")
            .field("buf", &self.buf)
            .finish_non_exhaustive()
    }
}

impl<F: UringFd> Completable for Write<F> {
    // The buffer and fd are `None` only when the op was cancelled and they were
    // kept in the driver; otherwise they are returned to the caller.
    type Output = (io::Result<u32>, Option<OwnedBuf>, Option<F>);
    fn complete(self, cqe: CqeResult) -> Self::Output {
        (cqe.result, Some(self.buf), Some(self.fd))
    }

    fn complete_with_error(self, err: Error) -> Self::Output {
        (Err(err), Some(self.buf), Some(self.fd))
    }

    fn complete_after_cancel(err: Error) -> Self::Output {
        (Err(err), None, None)
    }
}

impl Cancellable for Write {
    fn cancel(self) -> CancelData {
        CancelData::Write(self)
    }
}

impl Op<Write> {
    /// Issue a write that starts at `buf_offset` within `buf` and writes some bytes
    /// into `file` at `file_offset`.
    pub(crate) fn write_at(
        fd: OwnedFd,
        buf: OwnedBuf,
        buf_offset: usize,
        file_offset: u64,
    ) -> io::Result<Self> {
        let write = Write::new(fd, buf);
        let sqe = write.sqe(buf_offset, file_offset);

        // SAFETY: parameters of the entry, such as `fd` and `buf`, are valid
        // until this operation completes.
        let op = unsafe { Op::new(sqe, write) };
        Ok(op)
    }
}
