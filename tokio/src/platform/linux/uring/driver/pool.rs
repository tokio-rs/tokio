use crate::driver;

use io_uring::{opcode, IoUring};
use std::io;
use std::mem::ManuallyDrop;

/// Buffer pool shared with kernel
pub(crate) struct Pool {
    mem: *mut u8,
    num: usize,
    size: usize,
}

pub(crate) struct ProvidedBuf {
    buf: ManuallyDrop<Vec<u8>>,
    driver: driver::Handle,
}

impl Pool {
    pub(super) fn new(num: usize, size: usize) -> Pool {
        let total = num * size;
        let mut mem = ManuallyDrop::new(Vec::<u8>::with_capacity(total));

        assert_eq!(mem.capacity(), total);

        Pool {
            mem: mem.as_mut_ptr(),
            num,
            size,
        }
    }

    pub(super) fn provide_buffers(&self, uring: &mut IoUring) -> io::Result<()> {
        let op = opcode::ProvideBuffers::new(self.mem, self.size as _, self.num as _, 0, 0)
            .build()
            .user_data(0);

        // Scoped to ensure `sq` drops before trying to submit
        {
            let mut sq = uring.submission();

            if unsafe { sq.push(&op) }.is_err() {
                unimplemented!("when is this hit?");
            }
        }

        uring.submit_and_wait(1)?;

        let mut cq = uring.completion();
        for cqe in &mut cq {
            assert_eq!(cqe.user_data(), 0);
        }

        Ok(())
    }
}

impl ProvidedBuf {}

impl Drop for ProvidedBuf {
    fn drop(&mut self) {
        let mut driver = self.driver.borrow_mut();
        let pool = &driver.pool;

        let ptr = self.buf.as_mut_ptr();
        let bid = (ptr as usize - pool.mem as usize) / pool.size;

        let op = opcode::ProvideBuffers::new(ptr, pool.size as _, 1, 0, bid as _)
            .build()
            .user_data(u64::MAX);

        let mut sq = driver.uring.submission();

        if unsafe { sq.push(&op) }.is_err() {
            unimplemented!();
        }
    }
}
