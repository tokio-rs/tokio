use crate::fs::read_uring::MAX_READ_SIZE;
use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};

use io_uring::squeue::{Entry, Flags};
use io_uring::{opcode, types};
use std::io::ErrorKind;
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

        if let CqeResult::Single(Ok(len)) = cqe {
            // increase length of buffer on successful
            // completion
            let new_len = buf.len() + len as usize;
            // SAFETY: Kernel read len bytes
            unsafe { buf.set_len(new_len) };
        }

        // Handle rest each batch outside

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

    // Split file read operations by batches of size batch_size. batches will be executed in groupts
    // of batch_size
    pub(crate) async fn read_batch_size(
        mut fd: OwnedFd,
        mut buf: Vec<u8>,
        len: usize,
        batch_size: usize,
    ) -> Result<Vec<u8>, (OwnedFd, Vec<u8>)> {
        // hold multiple >= batch_size length vectors of operations
        let mut batches_of_ops = Vec::new();
        // hold batch_size operations and reuse it
        let mut n_ops = Vec::with_capacity(size_of::<Entry>() * batch_size);

        // total number of batch entries to read the file completly
        for (index, start) in (0..len).step_by(MAX_READ_SIZE).enumerate() {
            if (index + 1) % batch_size == 0 {
                batches_of_ops.push(n_ops.clone());
                n_ops.clear();
            } else {
                let end = (start + MAX_READ_SIZE).min(len); // clamp to len for the final chunk
                                                            // MAX_READ_SIZE is less than u32
                let len = (end - start) as u32;

                let buf_mut_ptr = buf.spare_capacity_mut()[start..].as_mut_ptr().cast();

                let op = opcode::Read::new(types::Fd(fd.as_raw_fd()), buf_mut_ptr, len)
                    .offset(start as u64)
                    .build()
                    .flags(Flags::IO_LINK);

                n_ops.push(op);
            }
        }

        // push out the last batches
        batches_of_ops.push(n_ops.clone());

        let mut read_size = 0;

        for batches in batches_of_ops {
            let op = unsafe { Op::batch(batches, Read { fd, buf }) };
            // TODO: Maybe we can put this in a tokio task
            let (res, r_fd, r_buf) = op.await;

            match res {
                CqeResult::Batch(cqes) => {
                    for cqe in cqes {
                        match cqe {
                            Ok(r_size) => {
                                read_size += r_size as usize;
                            }
                            Err(e) => {
                                if e.kind() != ErrorKind::Interrupted {
                                    return Err((r_fd, r_buf));
                                }
                            }
                        }
                    }
                }
                _ => return Err((r_fd, r_buf)),
            };

            fd = r_fd;
            buf = r_buf;
        }

        // SAFETY: We have read `read_size` amount
        unsafe {
            buf.set_len(buf.len() + read_size);
        }

        Ok(buf)
    }
}
