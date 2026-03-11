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
        let mut buf = self.buf;

        if let Ok(len) = cqe.result {
            let new_len = buf.len() + len as usize;
            // SAFETY: Kernel read len bytes
            unsafe { buf.set_len(new_len) };
        }

        (cqe.result, self.fd, buf)
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
    // dynamic buffer with uninitialized memory. The read happens on uninitialized
    // buffer and no overwriting happens.

    // SAFETY: The `len` of the amount to be read and the buffer that is passed
    // should have capacity > len.
    //
    // If `len` read is higher than vector capacity then setting its length by
    // the caller in terms of size_read can be unsound.
    pub(crate) fn read(fd: OwnedFd, mut buf: Vec<u8>, len: u32, offset: u64) -> Self {
        // don't overwrite on already written part
        assert!(buf.spare_capacity_mut().len() >= len as usize);
        let buf_mut_ptr = buf.spare_capacity_mut().as_mut_ptr().cast();

        let read_op = opcode::Read::new(types::Fd(fd.as_raw_fd()), buf_mut_ptr, len)
            .offset(offset)
            .build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        unsafe { Op::new(read_op, Read { fd, buf }) }
    }
}
