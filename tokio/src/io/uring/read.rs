use crate::fs::read_uring::MAX_READ_SIZE;
use crate::runtime::driver::op::{
    i32_to_result, CancelData, Cancellable, Completable, CqeResult, Op,
};

use io_uring::squeue::{Entry, Flags};
use io_uring::{opcode, types};

use std::io::ErrorKind;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, OwnedFd};

type Output<const N: usize> = (CqeResult<N>, OwnedFd, Vec<u8>);

#[derive(Debug)]
pub(crate) struct Read {
    fd: OwnedFd,
    buf: Vec<u8>,
}

impl<const N: usize> Completable<N> for Read {
    type Output = Output<N>;

    fn complete(self, cqe: CqeResult<N>) -> Self::Output {
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

    // Split file read operations by batches of size N. batches will be executed
    // in groups of N.
    //
    // This function will return the final buffer of bytes in the file
    pub(crate) async fn read_batch_size<const N: usize>(
        mut fd: OwnedFd,
        mut buf: Vec<u8>,
    ) -> Result<Vec<u8>, (OwnedFd, Vec<u8>)> {
        // hold batch_size operations and reuse it
        let mut n_ops = [const { MaybeUninit::uninit() }; N];
        let mut last_len = 0;
        let mut read_len = 0;
        let len = buf.capacity();

        for i in 0..N {
            n_ops[i].write(opcode::Nop::new().build());
        }

        let mut n_ops = unsafe { assume_init(n_ops) };

        // total number of batch entries to read the file completly
        for (index, start) in (0..len).step_by(MAX_READ_SIZE).enumerate() {
            let end = (start + MAX_READ_SIZE).min(len); // clamp to len for the final chunk
            let array_idx = index % N;

            // skip first iteration
            if array_idx == 0 && index != 0 {
                // SAFETY: Batches are valid array entries
                let op = unsafe { Op::batch(n_ops.clone(), Read { fd, buf }) };
                let (_, r_fd, r_buf) = uring_task(op, &mut read_len).await?;

                fd = r_fd;
                buf = r_buf;
            } else {
                let op = opcode::Read::new(
                    types::Fd(fd.as_raw_fd()),
                    buf.spare_capacity_mut()[start..].as_mut_ptr().cast(),
                    (end - start) as u32,
                )
                .offset(start as u64)
                .build()
                // link our sqes so cqes arrive in order
                .flags(Flags::IO_LINK);

                n_ops[array_idx] = op;
            }

            last_len = array_idx; // save last array_idx for last batch
        }

        // Handle last partial batch if there is any
        if last_len > 0 {
            for (i, entry) in n_ops.iter_mut().enumerate() {
                // no op the double counted entries
                if i > last_len {
                    *entry = opcode::Nop::new().build();
                }
            }

            // SAFETY: Because of the no-op loop above, we can assume double entry
            // read is not possible
            let op = unsafe { Op::batch(n_ops, Read { fd, buf }) };

            let (_, _, r_buf) = uring_task(op, &mut read_len).await?;

            buf = r_buf;
        }

        unsafe {
            buf.set_len(buf.len() + read_len);
        }

        Ok(buf)
    }
}

unsafe fn assume_init<const N: usize>(n_ops: [MaybeUninit<Entry>; N]) -> [Entry; N] {
    unsafe { std::mem::transmute_copy(&n_ops) }
}

// Poll the batch operation and get the Output out of it
async fn uring_task<const N: usize>(
    op: Op<Read, N>,
    read_len: &mut usize,
) -> Result<Output<N>, (OwnedFd, Vec<u8>)> {
    let (res, r_fd, r_buf) = op.await;

    match res {
        CqeResult::Batch(cqes) => {
            for cqe in cqes {
                let cqe = i32_to_result(cqe);

                match cqe {
                    Ok(r_size) => match read_len.checked_add(r_size as usize) {
                        Some(len) => {
                            *read_len = len;
                        }
                        None => return Err((r_fd, r_buf)),
                    },
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

    Ok((res, r_fd, r_buf))
}
