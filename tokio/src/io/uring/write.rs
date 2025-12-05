use crate::io::blocking;
use crate::io::uring::utils::ArcFd;
use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};

use io_uring::{opcode, types};
use std::io::{self, Error};

pub(crate) struct Write {
    buf: blocking::Buf,
    fd: ArcFd,
}

impl std::fmt::Debug for Write {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Write")
            .field("buf_len", &self.buf.len())
            .field("fd", &self.fd.as_raw_fd())
            .finish()
    }
}

impl Completable for Write {
    type Output = (io::Result<u32>, blocking::Buf, ArcFd);
    fn complete(mut self, cqe: CqeResult) -> Self::Output {
        if let Ok(n) = cqe.result.as_ref() {
            self.buf.advance(*n as usize);
        }

        (cqe.result, self.buf, self.fd)
    }

    fn complete_with_error(self, err: Error) -> Self::Output {
        (Err(err), self.buf, self.fd)
    }
}

impl Cancellable for Write {
    fn cancel(self) -> CancelData {
        CancelData::Write(self)
    }
}

impl Op<Write> {
    /// Issue a write at `file_offset` from the provided `buf`. To use current file cursor, set `file_offset` to `-1` or `u64::MAX`.
    pub(crate) fn write_at(fd: ArcFd, buf: blocking::Buf, file_offset: u64) -> Self {
        // There is a cap on how many bytes we can write in a single uring write operation.
        // ref: https://github.com/axboe/liburing/discussions/497
        let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);

        let ptr = buf.bytes().as_ptr();

        let sqe = opcode::Write::new(types::Fd(fd.as_raw_fd()), ptr, len)
            .offset(file_offset)
            .build();

        // SAFETY: parameters of the entry, such as `fd` and `buf`, are valid
        // until this operation completes.
        unsafe { Op::new(sqe, Write { buf, fd }) }
    }
}
