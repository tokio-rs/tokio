use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};

use io_uring::{opcode, types};
use std::io::{self, Error};
use std::os::fd::{AsRawFd, OwnedFd};

#[derive(Debug)]
pub(crate) struct Read {
    fd: OwnedFd,
    buf: Vec<u8>,
}

impl Completable for Read {
    type Output = (io::Result<u32>, OwnedFd, Vec<u8>);

    fn complete(self, cqe: CqeResult) -> Self::Output {
        (cqe.result, self.fd, self.buf)
    }

    fn complete_with_error(self, err: Error) -> Self::Output {
        (Err(err), self.fd, self.buf)
    }
}

impl Cancellable for Read {
    fn cancel(self) -> CancelData {
        CancelData::Read(self)
    }
}

impl Op<Read> {
    // Submit a request to read a FD at given length and offset into a
    // dynamic buffer with uinitialized memory. The read happens on unitialized
    // buffer and no overwiting happens.

    // SAFETY: The `len` of the amount to be read and the buffer that is passed
    // should have capacity > len.
    //
    // If `len` read is higher than vector capacity then setting its length by
    // the caller in terms of size_read can be unsound.
    pub(crate) fn read(fd: OwnedFd, mut buf: Vec<u8>, len: u32, offset: u64) -> Self {
        // don't overwrite on already written part
        let written = buf.len();
        let slice: &mut [u8] = &mut buf[written..];
        let buf_mut_ptr = slice.as_mut_ptr();

        let read_op = opcode::Read::new(types::Fd(fd.as_raw_fd()), buf_mut_ptr, len)
            .offset(offset)
            .build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        unsafe { Op::new(read_op, Read { fd, buf }) }
    }
}
