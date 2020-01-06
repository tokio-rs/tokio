use libc::{gid_t, uid_t};

/// Credentials of a process
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct UCred {
    /// UID (user ID) of the process
    pub uid: uid_t,
    /// GID (group ID) of the process
    pub gid: gid_t,
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) use self::impl_linux::get_peer_cred;

#[cfg(any(
    target_os = "dragonfly",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub(crate) use self::impl_macos::get_peer_cred;

#[cfg(any(target_os = "solaris"))]
pub(crate) use self::impl_solaris::get_peer_cred;

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) mod impl_linux {
    use crate::net::unix::UnixStream;

    use libc::{c_void, getsockopt, socklen_t, SOL_SOCKET, SO_PEERCRED};
    use std::{io, mem};

    use libc::ucred;

    pub(crate) fn get_peer_cred(sock: &UnixStream) -> io::Result<super::UCred> {
        use std::os::unix::io::AsRawFd;

        unsafe {
            let raw_fd = sock.as_raw_fd();

            let mut ucred = ucred {
                pid: 0,
                uid: 0,
                gid: 0,
            };

            let ucred_size = mem::size_of::<ucred>();

            // These paranoid checks should be optimized-out
            assert!(mem::size_of::<u32>() <= mem::size_of::<usize>());
            assert!(ucred_size <= u32::max_value() as usize);

            let mut ucred_size = ucred_size as socklen_t;

            let ret = getsockopt(
                raw_fd,
                SOL_SOCKET,
                SO_PEERCRED,
                &mut ucred as *mut ucred as *mut c_void,
                &mut ucred_size,
            );
            if ret == 0 && ucred_size as usize == mem::size_of::<ucred>() {
                Ok(super::UCred {
                    uid: ucred.uid,
                    gid: ucred.gid,
                })
            } else {
                Err(io::Error::last_os_error())
            }
        }
    }
}

#[cfg(any(
    target_os = "dragonfly",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub(crate) mod impl_macos {
    use crate::net::unix::UnixStream;

    use libc::getpeereid;
    use std::io;
    use std::mem::MaybeUninit;
    use std::os::unix::io::AsRawFd;

    pub(crate) fn get_peer_cred(sock: &UnixStream) -> io::Result<super::UCred> {
        unsafe {
            let raw_fd = sock.as_raw_fd();

            let mut uid = MaybeUninit::uninit();
            let mut gid = MaybeUninit::uninit();

            let ret = getpeereid(raw_fd, uid.as_mut_ptr(), gid.as_mut_ptr());

            if ret == 0 {
                Ok(super::UCred {
                    uid: uid.assume_init(),
                    gid: gid.assume_init(),
                })
            } else {
                Err(io::Error::last_os_error())
            }
        }
    }
}

#[cfg(any(target_os = "solaris"))]
pub(crate) mod impl_solaris {
    use crate::net::unix::UnixStream;
    use std::io;
    use std::os::unix::io::AsRawFd;
    use std::ptr;

    #[allow(non_camel_case_types)]
    enum ucred_t {}

    extern "C" {
        fn ucred_free(cred: *mut ucred_t);
        fn ucred_geteuid(cred: *const ucred_t) -> super::uid_t;
        fn ucred_getegid(cred: *const ucred_t) -> super::gid_t;

        fn getpeerucred(fd: std::os::raw::c_int, cred: *mut *mut ucred_t) -> std::os::raw::c_int;
    }

    pub(crate) fn get_peer_cred(sock: &UnixStream) -> io::Result<super::UCred> {
        unsafe {
            let raw_fd = sock.as_raw_fd();

            let mut cred = ptr::null_mut::<*mut ucred_t>() as *mut ucred_t;

            let ret = getpeerucred(raw_fd, &mut cred);

            if ret == 0 {
                let uid = ucred_geteuid(cred);
                let gid = ucred_getegid(cred);

                ucred_free(cred);

                Ok(super::UCred { uid, gid })
            } else {
                Err(io::Error::last_os_error())
            }
        }
    }
}
