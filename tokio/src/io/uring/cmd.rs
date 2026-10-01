use crate::runtime::driver::op::{CancelData, Cancellable, Completable, CqeResult, Op};

use io_uring::{opcode, types};
use std::io::{self, Error};
use std::os::fd::{AsRawFd, OwnedFd};

#[derive(Debug)]
pub(crate) struct UringCmd {
    fd: OwnedFd,
}

impl Completable for UringCmd {
    type Output = (io::Result<u32>, OwnedFd);

    fn complete(self, cqe: CqeResult) -> Self::Output {
        (cqe.result, self.fd)
    }

    fn complete_with_error(self, err: Error) -> Self::Output {
        (Err(err), self.fd)
    }
}

impl Cancellable for UringCmd {
    fn cancel(self) -> CancelData {
        CancelData::UringCmd(self)
    }
}

impl Op<UringCmd> {
    pub(crate) fn uring_cmd16(
        fd: OwnedFd,
        cmd_op: u32,
        cmd: [u8; 16],
        buf_index: Option<u16>,
    ) -> Self {
        let uring_cmd = opcode::UringCmd16::new(types::Fd(fd.as_raw_fd()), cmd_op)
            .cmd(cmd)
            .buf_index(buf_index);
        let sqe = uring_cmd.build();

        // SAFETY: Parameters are valid for the entire duration of the operation
        unsafe { Op::new(sqe, UringCmd { fd }) }
    }
}
