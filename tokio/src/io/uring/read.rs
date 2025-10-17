use crate::fs::read_uring::MAX_READ_SIZE;
use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};

use io_uring::squeue::Flags;
use io_uring::{opcode, types};
use std::os::fd::{AsRawFd, OwnedFd};

#[derive(Debug)]
pub(crate) struct Read {
    fd: OwnedFd,
    buf: Vec<u8>,
}

impl Completable for Read {
    type Output = (CqeResult, OwnedFd, Vec<u8>);

    fn complete(self, cqe: CqeResult) -> Self::Output {
        let mut buf = self.buf;

        match cqe {
            // increase length of buffer on successful
            // completion
            CqeResult::Single(Ok(len)) => {
                let new_len = buf.len() + len as usize;
                // SAFETY: Kernel read len bytes
                unsafe { buf.set_len(new_len) };
            }
            _ => (),
        }

        (cqe, self.fd, buf)
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

    // Submit batch requests to read a FD, the function splits reads by MAX_READ_SIZE
    // and returns a list of CQEs which can be used to determine if read was successful
    // or not
    pub(crate) fn read_batch(fd: OwnedFd, mut buf: Vec<u8>, len: usize) -> Self {
        let entries = {
            // total number of batch entries to read the file completly
            let mut batch_entries = Vec::new();

            for start in (0..len).step_by(MAX_READ_SIZE) {
                let end = (start + MAX_READ_SIZE).min(len); // clamp to len for the final chunk
                                                            // MAX_READ_SIZE is less than u32
                let len = (end - start) as u32;

                let buf_mut_ptr = buf.spare_capacity_mut().as_mut_ptr().cast();

                let op = opcode::Read::new(types::Fd(fd.as_raw_fd()), buf_mut_ptr, len)
                    .offset(start as u64)
                    .build()
                    .flags(Flags::IO_LINK);

                batch_entries.push(op);
            }

            batch_entries
        };

        unsafe { Op::batch(entries, Read { fd, buf }) }
    }
}
