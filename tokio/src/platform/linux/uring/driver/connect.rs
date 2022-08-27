use crate::platform::linux::uring::driver::{Op, SharedFd};
use socket2::SockAddr;
use std::io;

/// Open a file
pub(crate) struct Connect {
    fd: SharedFd,
    socket_addr: SockAddr,
}

impl Op<Connect> {
    /// Submit a request to connect.
    pub(crate) fn connect(fd: &SharedFd, socket_addr: SockAddr) -> io::Result<Op<Connect>> {
        use io_uring::{opcode, types};

        Op::submit_with(
            Connect {
                fd: fd.clone(),
                socket_addr,
            },
            |connect| {
                opcode::Connect::new(
                    types::Fd(connect.fd.raw_fd()),
                    connect.socket_addr.as_ptr(),
                    connect.socket_addr.len(),
                )
                .build()
            },
        )
    }
}
