use std::io;
use std::os::unix::io::RawFd;

/// Result of a `recvmsg` call.
pub(crate) struct RecvMsgResult {
    /// Number of bytes of regular data read.
    pub(crate) bytes_read: usize,
    /// Number of valid bytes in the ancillary data buffer.
    pub(crate) ancillary_len: usize,
    /// Whether the ancillary data was truncated (`MSG_CTRUNC`).
    pub(crate) truncated: bool,
}

/// Perform a non-blocking `sendmsg` with vectored data and ancillary data.
pub(crate) fn sendmsg_vectored(
    fd: RawFd,
    iov: &[io::IoSlice<'_>],
    ancillary_buf: &[u8],
    ancillary_len: usize,
) -> io::Result<usize> {
    unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = iov.as_ptr() as *mut libc::iovec;
        msg.msg_iovlen = iov.len() as _;

        if ancillary_len > 0 {
            msg.msg_control = ancillary_buf.as_ptr() as *mut libc::c_void;
            msg.msg_controllen = ancillary_len as _;
        }

        let ret = libc::sendmsg(fd, &msg, 0);
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(ret as usize)
        }
    }
}

/// Perform a non-blocking `recvmsg` with vectored data and ancillary data.
pub(crate) fn recvmsg_vectored(
    fd: RawFd,
    iov: &mut [io::IoSliceMut<'_>],
    ancillary_buf: &mut [u8],
) -> io::Result<RecvMsgResult> {
    unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = iov.as_mut_ptr() as *mut libc::iovec;
        msg.msg_iovlen = iov.len() as _;
        msg.msg_control = ancillary_buf.as_mut_ptr() as *mut libc::c_void;
        msg.msg_controllen = ancillary_buf.len() as _;

        let flags = recv_flags();

        let ret = libc::recvmsg(fd, &mut msg, flags);
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            #[allow(clippy::unnecessary_cast)]
            let ancillary_len = msg.msg_controllen as usize;
            Ok(RecvMsgResult {
                bytes_read: ret as usize,
                ancillary_len,
                truncated: (msg.msg_flags & libc::MSG_CTRUNC) != 0,
            })
        }
    }
}

/// Returns flags for `recvmsg`. Uses `MSG_CMSG_CLOEXEC` where available
/// to atomically set close-on-exec on received file descriptors.
#[inline]
fn recv_flags() -> libc::c_int {
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
    ))]
    {
        libc::MSG_CMSG_CLOEXEC
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
    )))]
    {
        0
    }
}

/// Manually set `FD_CLOEXEC` on a file descriptor.
/// Used on platforms without `MSG_CMSG_CLOEXEC` (e.g., macOS).
#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd",
)))]
pub(crate) fn set_cloexec(fd: RawFd) -> io::Result<()> {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        let ret = libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
