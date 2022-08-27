use crate::platform::linux::uring::driver::{Op, SharedFd};

use std::io;

use io_uring::{opcode, types};

pub(crate) struct Fsync {
    fd: SharedFd,
}

impl Op<Fsync> {
    pub(crate) fn fsync(fd: &SharedFd) -> io::Result<Op<Fsync>> {
        Op::submit_with(Fsync { fd: fd.clone() }, |fsync| {
            opcode::Fsync::new(types::Fd(fsync.fd.raw_fd())).build()
        })
    }

    pub(crate) fn datasync(fd: &SharedFd) -> io::Result<Op<Fsync>> {
        Op::submit_with(Fsync { fd: fd.clone() }, |fsync| {
            opcode::Fsync::new(types::Fd(fsync.fd.raw_fd()))
                .flags(types::FsyncFlags::DATASYNC)
                .build()
        })
    }
}
